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
//! 3. Mesa defaults, unless already set: on a Qualcomm phone (`/dev/kgsl-3d0`,
//!    no DRM render node) EGL through Zink (`MESA_LOADER_DRIVER_OVERRIDE=zink`)
//!    on the payload's Turnip ICD (`VK_DRIVER_FILES`). Elsewhere (virtio-gpu
//!    on Cuttlefish, a PC) Mesa probes the render node itself; the simulator
//!    may force `GALLIUM_DRIVER=softpipe`.
//! 4. exec `usr/bin/aera-flutter` through the payload's own
//!    `ld-linux-*.so` with `--library-path usr/lib`, passing our arguments.
//!
//! Like flutter-pi and GTK, the embedder itself never touches mounts; this
//! is only the recovery-specific packaging step (spec/aerap.md).

use std::ffi::CString;
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
    // SAFETY (set_var): single-threaded, before exec.
    unsafe {
        let kgsl = Path::new("/dev/kgsl-3d0").exists();
        if kgsl && !set("MESA_LOADER_DRIVER_OVERRIDE") && !set("GALLIUM_DRIVER") {
            std::env::set_var("MESA_LOADER_DRIVER_OVERRIDE", "zink");
        }
        let icd = root.join("usr/share/vulkan/icd.d/freedreno_icd.json");
        if !set("VK_DRIVER_FILES") && icd.exists() {
            std::env::set_var("VK_DRIVER_FILES", icd);
        }
    }

    let loader = c(&root.join(LOADER));
    let mut argv = vec![
        loader.clone(),
        CString::new("--library-path").unwrap(),
        c(&root.join("usr/lib")),
        c(&root.join("usr/bin/aera-flutter")),
    ];
    argv.extend(std::env::args_os().skip(1).map(|a| CString::new(a.as_bytes()).unwrap()));
    let mut pointers: Vec<*const libc::c_char> = argv.iter().map(|a| a.as_ptr()).collect();
    pointers.push(std::ptr::null());
    // SAFETY: argv is NUL-terminated and outlives the call; the environment
    // (AERA_* variables, fds 3 and 4) passes through unchanged.
    unsafe { libc::execv(loader.as_ptr(), pointers.as_ptr()) };
    eprintln!("aera-plugin: exec {}: {}", loader.to_string_lossy(), std::io::Error::last_os_error());
    std::process::exit(127);
}
