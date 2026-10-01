//! `usr/bin/aera-plugin`: what AERA execs. Built static (no loader needed),
//! it prepares the process and execs the glibc embedder.
//!
//! 0. stdout and stderr to `$AERA_PLUGIN_DATA/aera-flutter.log`, new each
//!    launch: AERA leaves them on recovery's own streams, where nobody reads
//!    them (flutter-pi logs to the journal for the same reason).
//! 1. A private mount namespace, so nothing below leaks into recovery.
//! 2. AERA's fonts (`/twres/fonts`) at `/usr/share/fonts`, where the engine
//!    looks, and the payload's CA bundle at `/etc/ssl/certs`, which Dart
//!    trusts. A failed bind is logged, not fatal: text or TLS degrade, the
//!    app still starts.
//! 3. Mesa defaults, unless already set: the payload's Vulkan ICDs (Turnip on
//!    arm64, gfxstream on x64) in `VK_DRIVER_FILES`, and EGL through Zink
//!    (`MESA_LOADER_DRIVER_OVERRIDE=zink`) where the GPU only speaks Vulkan:
//!    a Qualcomm phone (`/dev/kgsl-3d0`, no DRM render node) or a virtio-gpu
//!    that offers gfxstream or Venus Vulkan but no virgl (Cuttlefish's
//!    gfxstream modes). Elsewhere Mesa probes the render node itself; the
//!    simulator may force `GALLIUM_DRIVER=softpipe`.
//!    On that virtio-gpu the embedder is also started with `--vulkan`, since
//!    Zink on gfxstream does not start yet (EGL falls back to softpipe);
//!    `--gl` in the plugin's `engine-switches` overrides it.
//!    Developers can add variables (`MESA_DEBUG=1`, `EGL_LOG_LEVEL=debug`)
//!    one `KEY=VALUE` per line in `$AERA_PLUGIN_DATA/environment`.
//! 4. exec `usr/bin/aera-flutter` through the payload's own
//!    `ld-linux-*.so` with `--library-path usr/lib`, passing our arguments.
//!
//! Like flutter-pi and GTK, the embedder itself never touches mounts; this
//! is only the recovery-specific packaging step (spec/aerap.md).

use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

#[cfg(target_arch = "aarch64")]
const LOADER: &str = "usr/lib/ld-linux-aarch64.so.1";
#[cfg(target_arch = "x86_64")]
const LOADER: &str = "usr/lib/ld-linux-x86-64.so.2";

fn c(path: &Path) -> CString {
    CString::new(path.as_os_str().as_bytes()).expect("no NUL in paths")
}

fn bind(from: &Path, to: &Path) {
    if !from.exists() {
        eprintln!("aera-plugin: {} missing, not bound", from.display());
        return;
    }
    let _ = std::fs::create_dir_all(to);
    // SAFETY: valid NUL-terminated paths; a bind mount in our own namespace.
    let rc = unsafe {
        libc::mount(c(from).as_ptr(), c(to).as_ptr(), std::ptr::null(), libc::MS_BIND | libc::MS_REC, std::ptr::null())
    };
    if rc != 0 {
        eprintln!("aera-plugin: bind {} -> {}: {}", from.display(), to.display(), std::io::Error::last_os_error());
    }
}

const RENDER_NODE: &CStr = c"/dev/dri/renderD128";
// virtgpu_drm.h: DRM_IOCTL_VIRTGPU_GETPARAM, VIRTGPU_PARAM_SUPPORTED_CAPSET_IDs
// and the capset ids.
const VIRTGPU_GETPARAM: libc::c_ulong = 0xC010_6443;
const PARAM_SUPPORTED_CAPSET_IDS: u64 = 7;
const CAPSET_VIRGL: u32 = 1 << 1 | 1 << 2;
const CAPSET_VULKAN: u32 = 1 << 3 | 1 << 4; // gfxstream Vulkan, Venus

/// The virtio-gpu capsets the render node offers, as a bit mask; `None` if
/// there is no virtio-gpu render node.
fn virtio_capsets() -> Option<u32> {
    #[repr(C)]
    struct GetParam {
        param: u64,
        value: u64,
    }
    let mut mask: u32 = 0;
    // SAFETY: open/ioctl/close on a device node with a struct of the size
    // the ioctl number encodes; the kernel writes an int to `value`.
    unsafe {
        let fd = libc::open(RENDER_NODE.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC);
        if fd < 0 {
            return None;
        }
        let mut p = GetParam { param: PARAM_SUPPORTED_CAPSET_IDS, value: &mut mask as *mut u32 as u64 };
        let rc = libc::ioctl(fd, VIRTGPU_GETPARAM as _, &mut p);
        libc::close(fd);
        (rc == 0).then_some(mask)
    }
}

fn kgsl_present() -> bool {
    Path::new("/dev/kgsl-3d0").exists()
}

/// Whether EGL should go through Zink: the GPU is only reachable through
/// Vulkan.
fn wants_zink(kgsl: bool, capsets: Option<u32>) -> bool {
    kgsl || capsets.is_some_and(|m| m & CAPSET_VIRGL == 0 && m & CAPSET_VULKAN != 0)
}

/// Every Vulkan ICD in the payload, for `VK_DRIVER_FILES`.
fn payload_icds(root: &Path) -> Option<std::ffi::OsString> {
    let mut icds: Vec<PathBuf> = std::fs::read_dir(root.join("usr/share/vulkan/icd.d"))
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    icds.sort();
    let joined = std::env::join_paths(icds).ok()?;
    (!joined.is_empty()).then_some(joined)
}

/// `KEY=VALUE` lines; blank lines and `#` comments are skipped.
fn parse_environment(text: &str) -> Vec<(String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned())))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

fn root() -> PathBuf {
    if let Some(r) = std::env::var_os("AERA_PLUGIN_ROOT") {
        return r.into();
    }
    let exe = std::env::current_exe().unwrap_or_default();
    exe.ancestors().nth(3).unwrap_or(Path::new("/")).to_path_buf()
}

/// Sends stdout and stderr to the plugin's log file.
fn log_to_data_dir() {
    let Some(data) = std::env::var_os("AERA_PLUGIN_DATA") else { return };
    let path = c(&Path::new(&data).join("aera-flutter.log"));
    // SAFETY: a valid path; the new descriptor replaces 1 and 2 and the
    // original is closed.
    unsafe {
        let fd = libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC | libc::O_CLOEXEC, 0o600);
        if fd >= 0 {
            libc::dup2(fd, 1);
            libc::dup2(fd, 2);
            libc::close(fd);
        }
    }
}

fn main() {
    log_to_data_dir();
    let root = root();
    // SAFETY: plain syscalls with constant arguments.
    let private = unsafe {
        libc::unshare(libc::CLONE_NEWNS) == 0
            && libc::mount(c"none".as_ptr(), c"/".as_ptr(), std::ptr::null(), libc::MS_REC | libc::MS_PRIVATE, std::ptr::null()) == 0
    };
    if private {
        bind(Path::new("/twres/fonts"), Path::new("/usr/share/fonts"));
        bind(&root.join("etc/ssl/certs"), Path::new("/etc/ssl/certs"));
    } else {
        eprintln!("aera-plugin: no private mount namespace ({}); fonts and CA bundle not bound", std::io::Error::last_os_error());
    }

    let set = |k: &str| std::env::var_os(k).is_some();
    let capsets = virtio_capsets();
    // SAFETY (set_var): single-threaded, before exec.
    unsafe {
        let kgsl = kgsl_present();
        if let Some(m) = capsets {
            eprintln!("aera-plugin: virtio-gpu capsets {m:#x}");
        }
        if let Some(data) = std::env::var_os("AERA_PLUGIN_DATA") {
            if let Ok(text) = std::fs::read_to_string(Path::new(&data).join("environment")) {
                for (k, v) in parse_environment(&text) {
                    eprintln!("aera-plugin: {k}={v}");
                    std::env::set_var(k, v);
                }
            }
        }
        if wants_zink(kgsl, capsets) && !set("MESA_LOADER_DRIVER_OVERRIDE") && !set("GALLIUM_DRIVER") {
            std::env::set_var("MESA_LOADER_DRIVER_OVERRIDE", "zink");
        }
        if let Some(icds) = payload_icds(&root).filter(|_| !set("VK_DRIVER_FILES")) {
            std::env::set_var("VK_DRIVER_FILES", icds);
        }
    }

    let loader = c(&root.join(LOADER));
    let mut argv = vec![
        loader.clone(),
        CString::new("--library-path").unwrap(),
        c(&root.join("usr/lib")),
        c(&root.join("usr/bin/aera-flutter")),
    ];
    if !kgsl_present() && wants_zink(false, capsets) {
        argv.push(CString::new("--vulkan").unwrap());
    }
    argv.extend(std::env::args_os().skip(1).map(|a| CString::new(a.as_bytes()).unwrap()));
    let mut pointers: Vec<*const libc::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    pointers.push(std::ptr::null());
    // SAFETY: argv is NUL-terminated and outlives the call; the environment
    // (AERA_* variables, fds 3 and 4) passes through unchanged.
    unsafe { libc::execv(loader.as_ptr(), pointers.as_ptr()) };
    eprintln!("aera-plugin: exec {}: {}", loader.to_string_lossy(), std::io::Error::last_os_error());
    std::process::exit(127);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zink_only_without_a_gl_path() {
        assert!(wants_zink(true, None));
        assert!(!wants_zink(false, None));
        // Cuttlefish gfxstream: Vulkan (3), composer (9), GLES (8).
        assert!(wants_zink(false, Some(1 << 3 | 1 << 8 | 1 << 9)));
        // drm_virgl: virgl and virgl2.
        assert!(!wants_zink(false, Some(1 << 1 | 1 << 2)));
        // Virgl beside Venus: Mesa's virgl stays.
        assert!(!wants_zink(false, Some(1 << 2 | 1 << 4)));
        assert!(!wants_zink(false, Some(0)));
    }

    #[test]
    fn environment_file() {
        let env = parse_environment("# debug\nMESA_DEBUG=1\n\n EGL_LOG_LEVEL = debug \nnot a pair\n=x\n");
        assert_eq!(env, [("MESA_DEBUG".into(), "1".into()), ("EGL_LOG_LEVEL".into(), "debug".into())]);
    }
}
