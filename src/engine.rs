//! Hosting `libflutter_engine.so`: load it, describe the project, run the
//! platform loop. ≈ GTK's `fl_engine` plus flutter-pi's main loop.
//!
//! Threads: the caller's thread is Flutter's platform thread and runs
//! [`Engine::run`]: platform tasks, AERA's control socket and vsync answers.
//! The engine's own raster thread calls the renderer callbacks and presents
//! frames into AERA's slots.

use std::ffi::{c_char, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use libloading::Library;

use crate::ffi::{self, FlutterEngineProcTable, FlutterTask, FLUTTER_ENGINE_VERSION};
use crate::handlers::{self, Effect, Handlers};
use crate::host::{kind, lifecycle, Control, Message, Slots};
use crate::renderer::gl::{self, Gl};
use crate::renderer::vk::Vk;
use crate::renderer::Renderer;
use crate::task_runner::TaskRunner;
use crate::view::View;

/// Where the engine finds the app, normally inside the payload.
#[derive(Clone, Debug)]
pub struct Config {
    pub engine_library: PathBuf,
    pub assets: PathBuf,
    pub icu_data: PathBuf,
    /// `libapp.so`, used when the engine is an AOT (profile/release) build.
    pub aot_library: PathBuf,
    /// Extra engine switches, e.g. `--verbose-logging`.
    pub engine_args: Vec<String>,
    /// Render with Vulkan instead of GL (`--vulkan`).
    pub vulkan: bool,
}

impl Config {
    /// The payload layout in spec/aerap.md.
    pub fn from_root(root: &Path) -> Config {
        Config {
            engine_library: root.join("usr/lib/libflutter_engine.so"),
            assets: root.join("usr/share/flutter/flutter_assets"),
            icu_data: root.join("usr/share/flutter/icudtl.dat"),
            aot_library: root.join("usr/lib/libapp.so"),
            engine_args: vec![],
            vulkan: false,
        }
    }
}

struct Vsync {
    baton: Option<isize>,
    last_ns: u64,
}

/// Everything the engine's callbacks reach through `user_data`.
struct Shared {
    procs: FlutterEngineProcTable,
    engine: Mutex<ffi::FlutterEngine>,
    control: Control,
    slots: Slots,
    renderer: Renderer,
    runner: TaskRunner,
    view: Mutex<View>,
    handlers: Mutex<Handlers>,
    vsync: Mutex<Vsync>,
    exit: AtomicBool,
    frames: std::sync::atomic::AtomicU64,
}

// SAFETY: the engine handle and proc table are thread-safe per
// flutter_embedder.h for the calls made from other threads (OnVsync,
// PostRenderThreadTask aside, we only call from the platform thread);
// everything else is behind locks or atomics.
unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

pub struct Engine {
    _library: Library,
    shared: Box<Shared>,
    aot: ffi::FlutterEngineAOTData,
    // Kept alive for the engine's lifetime: it holds pointers into these.
    _strings: Vec<CString>,
}

fn load_procs(library: &Library) -> Result<FlutterEngineProcTable, String> {
    // SAFETY: the symbol has this signature in flutter_embedder.h; the table
    // is a plain C struct filled by the engine.
    unsafe {
        let get: libloading::Symbol<unsafe extern "C" fn(*mut FlutterEngineProcTable) -> ffi::FlutterEngineResult> =
            library.get(b"FlutterEngineGetProcAddresses\0").map_err(|e| e.to_string())?;
        let mut procs: FlutterEngineProcTable = std::mem::zeroed();
        procs.struct_size = std::mem::size_of::<FlutterEngineProcTable>();
        if get(&mut procs) != ffi::kSuccess {
            return Err("FlutterEngineGetProcAddresses failed".into());
        }
        Ok(procs)
    }
}

fn cstring(s: impl AsRef<std::ffi::OsStr>) -> CString {
    use std::os::unix::ffi::OsStrExt;
    CString::new(s.as_ref().as_bytes()).expect("no NUL in paths")
}

/// The locale from `LC_ALL`/`LANG` (`en_US.UTF-8` → `en`, `US`), defaulting
/// to `en-US` as recovery usually has no locale set.
pub fn locale_from_env(value: Option<&str>) -> (String, Option<String>) {
    let v = value.unwrap_or("").split(['.', '@']).next().unwrap_or("");
    if v.is_empty() || v == "C" || v == "POSIX" {
        return ("en".into(), Some("US".into()));
    }
    match v.split_once('_') {
        Some((l, c)) => (l.to_owned(), Some(c.to_owned())),
        None => (v.to_owned(), None),
    }
}

impl Engine {
    /// Loads the engine and starts the app. `early` are host messages that
    /// arrived during the handshake.
    pub fn start(config: Config, control: Control, slots: Slots) -> Result<Engine, String> {
        let geometry = slots.geometry();
        // SAFETY: loading the engine library we ship; its initializers are
        // the engine's own.
        let library = unsafe { Library::new(&config.engine_library) }
            .map_err(|e| format!("load {}: {e}", config.engine_library.display()))?;
        let procs = load_procs(&library)?;
        // The engine's switch, as GTK passes it through.
        let impeller = config.engine_args.iter().any(|a| a == "--enable-impeller" || a == "--enable-impeller=true");
        let renderer = if config.vulkan {
            match Vk::new(geometry.width, geometry.height, geometry.stride as usize) {
                Ok(v) => {
                    eprintln!("aera-flutter: Vulkan on {}", v.name());
                    Renderer::Vk(v)
                }
                Err(e) => {
                    eprintln!("aera-flutter: Vulkan unavailable ({e}), using GL");
                    Renderer::Gl(Gl::new(geometry.width, geometry.height, impeller)?)
                }
            }
        } else {
            Renderer::Gl(Gl::new(geometry.width, geometry.height, impeller)?)
        };
        let shared = Box::new(Shared {
            procs,
            engine: Mutex::new(std::ptr::null_mut()),
            control,
            slots,
            renderer,
            runner: TaskRunner::new().map_err(|e| e.to_string())?,
            view: Mutex::new(View::new(geometry)),
            handlers: Mutex::new(Handlers::default()),
            vsync: Mutex::new(Vsync { baton: None, last_ns: 0 }),
            exit: AtomicBool::new(false),
            frames: Default::default(),
        });
        let user_data = &*shared as *const Shared as *mut c_void;

        // SAFETY: plain C structs; zeroed is valid and every callback we set
        // matches the header's signature.
        // SAFETY: plain C structs; zeroed is valid and every callback we set
        // matches the header's signature.
        let mut renderer: ffi::FlutterRendererConfig = unsafe { std::mem::zeroed() };
        // Extension name pointers, read during Initialize only.
        let mut vk_names: (Vec<*const c_char>, Vec<*const c_char>) = (vec![], vec![]);
        match &shared.renderer {
            Renderer::Gl(_) => unsafe {
                renderer.type_ = ffi::kOpenGL;
                let gl = &mut renderer.__bindgen_anon_1.open_gl;
                gl.struct_size = std::mem::size_of::<ffi::FlutterOpenGLRendererConfig>();
                gl.make_current = Some(cb_make_current);
                gl.clear_current = Some(cb_clear_current);
                gl.make_resource_current = Some(cb_make_resource_current);
                gl.present = Some(cb_present);
                gl.fbo_callback = Some(cb_fbo);
                gl.surface_transformation = Some(cb_transformation);
                gl.gl_proc_resolver = Some(cb_proc_resolver);
            },
            Renderer::Vk(v) => unsafe {
                vk_names.0 = v.instance_extensions.iter().map(|e| e.as_ptr()).collect();
                vk_names.1 = v.device_extensions.iter().map(|e| e.as_ptr()).collect();
                renderer.type_ = ffi::kVulkan;
                let c = &mut renderer.__bindgen_anon_1.vulkan;
                c.struct_size = std::mem::size_of::<ffi::FlutterVulkanRendererConfig>();
                c.version = v.api_version;
                c.instance = v.instance_handle();
                c.physical_device = v.physical_device_handle();
                c.device = v.device_handle();
                c.queue_family_index = v.queue_family;
                c.queue = v.queue_handle();
                c.enabled_instance_extension_count = vk_names.0.len();
                c.enabled_instance_extensions = vk_names.0.as_mut_ptr();
                c.enabled_device_extension_count = vk_names.1.len();
                c.enabled_device_extensions = vk_names.1.as_mut_ptr();
                c.get_instance_proc_address_callback = Some(cb_vk_proc);
                c.get_next_image_callback = Some(cb_vk_next_image);
                c.present_image_callback = Some(cb_vk_present);
            },
        }

        let mut strings = vec![cstring("aera-flutter")];
        strings.extend(config.engine_args.iter().map(cstring));
        let argc = strings.len();
        let assets = cstring(&config.assets);
        let icu = cstring(&config.icu_data);
        strings.push(assets);
        strings.push(icu);
        let argv: Vec<*const c_char> = strings[..argc].iter().map(|s| s.as_ptr()).collect();

        let platform_runner = ffi::FlutterTaskRunnerDescription {
            struct_size: std::mem::size_of::<ffi::FlutterTaskRunnerDescription>(),
            user_data,
            runs_task_on_current_thread_callback: Some(cb_runs_on_platform),
            post_task_callback: Some(cb_post_task),
            identifier: 1,
            destruction_callback: None,
        };
        // SAFETY: as above.
        let mut runners: ffi::FlutterCustomTaskRunners = unsafe { std::mem::zeroed() };
        runners.struct_size = std::mem::size_of::<ffi::FlutterCustomTaskRunners>();
        runners.platform_task_runner = &platform_runner;

        let mut aot: ffi::FlutterEngineAOTData = std::ptr::null_mut();
        // SAFETY: proc table calls with valid arguments.
        if unsafe { (procs.RunsAOTCompiledDartCode.unwrap())() } {
            let elf = cstring(&config.aot_library);
            let source = ffi::FlutterEngineAOTDataSource {
                type_: ffi::kFlutterEngineAOTDataSourceTypeElfPath,
                __bindgen_anon_1: ffi::FlutterEngineAOTDataSource__bindgen_ty_1 { elf_path: elf.as_ptr() },
            };
            if unsafe { (procs.CreateAOTData.unwrap())(&source, &mut aot) } != ffi::kSuccess {
                return Err(format!("could not load AOT data from {}", config.aot_library.display()));
            }
        }

        // SAFETY: as above.
        let mut args: ffi::FlutterProjectArgs = unsafe { std::mem::zeroed() };
        args.struct_size = std::mem::size_of::<ffi::FlutterProjectArgs>();
        args.assets_path = strings[argc].as_ptr();
        args.icu_data_path = strings[argc + 1].as_ptr();
        args.command_line_argc = argc as i32;
        args.command_line_argv = argv.as_ptr();
        args.platform_message_callback = Some(cb_platform_message);
        args.vsync_callback = Some(cb_vsync);
        args.custom_task_runners = &runners;
        args.shutdown_dart_vm_when_done = true;
        args.aot_data = aot;
        args.log_message_callback = Some(cb_log);
        args.compute_platform_resolved_locale_callback = Some(cb_resolve_locale);

        let mut engine: ffi::FlutterEngine = std::ptr::null_mut();
        // SAFETY: everything `args` points at outlives this call; the engine
        // copies what it keeps. `shared` is boxed and outlives the engine.
        let result = unsafe {
            (procs.Initialize.unwrap())(FLUTTER_ENGINE_VERSION as usize, &renderer, &args, user_data, &mut engine)
        };
        if result != ffi::kSuccess {
            return Err(format!("FlutterEngineInitialize failed ({result})"));
        }
        *shared.engine.lock().unwrap() = engine;
        // SAFETY: an initialized engine.
        let result = unsafe { (procs.RunInitialized.unwrap())(engine) };
        if result != ffi::kSuccess {
            return Err(format!("FlutterEngineRunInitialized failed ({result})"));
        }
        let me = Engine { _library: library, shared, aot, _strings: strings };
        // After run, as GTK does: the shell must exist (it crashes before).
        let display = me.shared.view.lock().unwrap().display();
        // SAFETY: a live engine and a display struct on the stack.
        unsafe {
            (procs.NotifyDisplayUpdate.unwrap())(me.shared.engine(), ffi::kFlutterEngineDisplaysUpdateTypeStartup, &display, 1);
        }
        me.send_locales();
        me.shared.send_metrics();
        me.shared.apply(vec![handlers::settings::initial(), handlers::lifecycle::message("resumed")]);
        Ok(me)
    }

    fn send_locales(&self) {
        let lang = std::env::var("LC_ALL").or_else(|_| std::env::var("LANG")).ok();
        let (language, country) = locale_from_env(lang.as_deref());
        let language = cstring(language);
        let country = country.map(cstring);
        let locale = ffi::FlutterLocale {
            struct_size: std::mem::size_of::<ffi::FlutterLocale>(),
            language_code: language.as_ptr(),
            country_code: country.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
            script_code: std::ptr::null(),
            variant_code: std::ptr::null(),
        };
        let mut list = [&locale as *const ffi::FlutterLocale];
        // SAFETY: a live engine; the locale strings outlive the call.
        unsafe { (self.shared.procs.UpdateLocales.unwrap())(self.shared.engine(), list.as_mut_ptr(), 1) };
    }

    /// Replays host messages that arrived before the engine started.
    pub fn replay(&self, early: Vec<Message>) {
        for m in early {
            self.shared.on_host(&m);
        }
    }

    /// The platform loop. Returns when the app or AERA ends the session.
    pub fn run(self) -> i32 {
        let s = &*self.shared;
        let mut status = 0;
        while !s.exit.load(Ordering::Acquire) {
            let now = s.now();
            for task in s.runner.take_due(now) {
                // SAFETY: a task the engine posted to this runner, run once
                // on its thread.
                unsafe { (s.procs.RunTask.unwrap())(s.engine(), &task) };
            }
            let vsync_wait = s.answer_vsync(s.now());
            let task_wait = s.runner.next_target().map(|t| t.saturating_sub(s.now()));
            let wait_ns = [vsync_wait, task_wait].into_iter().flatten().min();
            let timeout = wait_ns.map_or(-1, |ns| ns.div_ceil(1_000_000).min(i32::MAX as u64) as i32);
            let mut fds = [
                libc::pollfd { fd: s.control.raw_fd(), events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: s.runner.wake_fd(), events: libc::POLLIN, revents: 0 },
            ];
            // SAFETY: two valid pollfds.
            if unsafe { libc::poll(fds.as_mut_ptr(), 2, timeout) } < 0 {
                continue;
            }
            if fds[1].revents != 0 {
                s.runner.drain_wake();
            }
            if fds[0].revents != 0 {
                loop {
                    match s.control.try_recv() {
                        Ok(Some(m)) => s.on_host(&m),
                        Ok(None) => break,
                        Err(e) => {
                            eprintln!("aera-flutter: host gone ({e})");
                            s.exit.store(true, Ordering::Release);
                            status = 1;
                            break;
                        }
                    }
                }
            }
        }
        eprintln!("aera-flutter: shutting down after {} frames", s.frames.load(Ordering::Relaxed));
        status
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let s = &*self.shared;
        s.slots.close();
        let engine = s.engine();
        if !engine.is_null() {
            // SAFETY: shut down once, on the platform thread, before
            // `shared` is freed.
            unsafe { (s.procs.Shutdown.unwrap())(engine) };
        }
        if !self.aot.is_null() {
            // SAFETY: created by CreateAOTData, collected once after shutdown.
            unsafe { (s.procs.CollectAOTData.unwrap())(self.aot) };
        }
    }
}

impl Shared {
    fn engine(&self) -> ffi::FlutterEngine {
        *self.engine.lock().unwrap()
    }

    fn now(&self) -> u64 {
        // SAFETY: a pure clock read.
        unsafe { (self.procs.GetCurrentTime.unwrap())() }
    }

    fn send_metrics(&self) {
        let m = self.view.lock().unwrap().metrics();
        // SAFETY: a live engine and a metrics struct on the stack.
        unsafe { (self.procs.SendWindowMetricsEvent.unwrap())(self.engine(), &m) };
    }

    fn send(&self, channel: &str, bytes: &[u8]) {
        let channel = CString::new(channel).unwrap();
        let message = ffi::FlutterPlatformMessage {
            struct_size: std::mem::size_of::<ffi::FlutterPlatformMessage>(),
            channel: channel.as_ptr(),
            message: bytes.as_ptr(),
            message_size: bytes.len(),
            response_handle: std::ptr::null(),
        };
        // SAFETY: a live engine; the engine copies the message.
        unsafe { (self.procs.SendPlatformMessage.unwrap())(self.engine(), &message) };
    }

    fn apply(&self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Host(m) => {
                    if let Err(e) = self.control.send(&m) {
                        eprintln!("aera-flutter: send to host: {e}");
                    }
                }
                Effect::Send { channel, bytes } => self.send(channel, &bytes),
                Effect::BottomInset(px) => {
                    self.view.lock().unwrap().bottom_inset = px;
                    self.send_metrics();
                }
                Effect::Exit => {
                    let _ = self.control.send(&Message::new(kind::CLOSE));
                    self.exit.store(true, Ordering::Release);
                }
            }
        }
    }

    fn on_host(&self, m: &Message) {
        match m.kind {
            kind::FRAME_DONE => {
                if !self.slots.frame_done(m.request_id) {
                    eprintln!("aera-flutter: FRAME_DONE for unknown frame {}", m.request_id);
                }
            }
            kind::TOUCH_DOWN | kind::TOUCH_MOVE | kind::TOUCH_UP => {
                let event = self.view.lock().unwrap().pointer(m, (self.now() / 1000) as usize);
                if let Some(e) = event {
                    // SAFETY: a live engine and one event on the stack.
                    unsafe { (self.procs.SendPointerEvent.unwrap())(self.engine(), &e, 1) };
                }
            }
            kind::SURFACE => eprintln!("aera-flutter: resize is not supported yet; keeping the first SURFACE"),
            _ => {
                let effects = self.handlers.lock().unwrap().on_host(m);
                self.apply(effects);
                if m.kind == kind::LIFECYCLE && m.value == lifecycle::STOP {
                    self.exit.store(true, Ordering::Release);
                }
            }
        }
    }

    /// Answers a pending vsync once a slot is free and a frame period has
    /// passed. Returns how long to wait before trying again, if anything is
    /// pending.
    fn answer_vsync(&self, now: u64) -> Option<u64> {
        let mut v = self.vsync.lock().unwrap();
        let baton = v.baton?;
        if !self.slots.has_free() {
            // FRAME_DONE wakes the loop through the control socket.
            return None;
        }
        let period = self.slots.geometry().frame_period().as_nanos() as u64;
        let earliest = v.last_ns + period;
        if now < earliest {
            return Some(earliest - now);
        }
        v.baton = None;
        v.last_ns = now;
        drop(v);
        // SAFETY: a baton from the vsync callback, answered once.
        unsafe { (self.procs.OnVsync.unwrap())(self.engine(), baton, now, now + period) };
        None
    }
}

/// # Safety
/// `user_data` is the `Shared` passed to `Initialize`, alive until shutdown.
unsafe fn shared<'a>(user_data: *mut c_void) -> &'a Shared {
    &*(user_data as *const Shared)
}

// The callbacks below never unwind: each body is plain calls that report
// failure by value, and the crate is built with panic = "abort" in release.

unsafe extern "C" fn cb_make_current(user_data: *mut c_void) -> bool {
    shared(user_data).renderer.gl().make_current()
}

unsafe extern "C" fn cb_clear_current(user_data: *mut c_void) -> bool {
    shared(user_data).renderer.gl().clear_current()
}

unsafe extern "C" fn cb_make_resource_current(user_data: *mut c_void) -> bool {
    shared(user_data).renderer.gl().make_resource_current()
}

unsafe extern "C" fn cb_fbo(user_data: *mut c_void) -> u32 {
    let s = shared(user_data);
    if s.frames.load(Ordering::Relaxed) == 0 {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| eprintln!("aera-flutter: GL renderer {}", s.renderer.gl().renderer()));
    }
    s.renderer.gl().framebuffer()
}

unsafe extern "C" fn cb_transformation(user_data: *mut c_void) -> ffi::FlutterTransformation {
    gl::flip_vertically(shared(user_data).slots.geometry().height)
}

unsafe extern "C" fn cb_proc_resolver(user_data: *mut c_void, name: *const c_char) -> *mut c_void {
    shared(user_data).renderer.gl().proc_address(CStr::from_ptr(name))
}

/// Raster thread: copy the frame into a free slot and tell AERA.
unsafe extern "C" fn cb_present(user_data: *mut c_void) -> bool {
    let s = shared(user_data);
    present(s, |slot, stride| s.renderer.gl().read_frame(slot, stride))
}

unsafe extern "C" fn cb_vk_proc(user_data: *mut c_void, instance: *mut c_void, name: *const c_char) -> *mut c_void {
    shared(user_data).renderer.vk().proc_address(instance, CStr::from_ptr(name))
}

unsafe extern "C" fn cb_vk_next_image(user_data: *mut c_void, _info: *const ffi::FlutterFrameInfo) -> ffi::FlutterVulkanImage {
    let v = shared(user_data).renderer.vk();
    ffi::FlutterVulkanImage {
        struct_size: std::mem::size_of::<ffi::FlutterVulkanImage>(),
        image: v.image_handle(),
        format: v.format.as_raw() as u32,
    }
}

unsafe extern "C" fn cb_vk_present(user_data: *mut c_void, _image: *const ffi::FlutterVulkanImage) -> bool {
    let s = shared(user_data);
    let v = s.renderer.vk();
    if s.frames.load(Ordering::Relaxed) == 0 {
        eprintln!("aera-flutter: Vulkan renderer {}", v.name());
    }
    present(s, |slot, stride| v.read_frame(slot, stride))
}

fn present(s: &Shared, read: impl FnOnce(&mut [u8], usize)) -> bool {
    let Some(index) = s.slots.acquire(Duration::from_secs(1)) else {
        // AERA held every slot for a second (or we are shutting down): drop
        // this frame rather than stall the raster thread forever.
        return true;
    };
    let stride = s.slots.geometry().stride as usize;
    // SAFETY: `index` is ours until `present`.
    read(unsafe { s.slots.slot_mut(index) }, stride);
    let message = s.slots.present(index);
    s.frames.fetch_add(1, Ordering::Relaxed);
    if let Err(e) = s.control.send(&message) {
        eprintln!("aera-flutter: PRESENT failed: {e}");
    }
    true
}

unsafe extern "C" fn cb_runs_on_platform(user_data: *mut c_void) -> bool {
    shared(user_data).runner.runs_on_current_thread()
}

unsafe extern "C" fn cb_post_task(task: FlutterTask, target_ns: u64, user_data: *mut c_void) {
    shared(user_data).runner.post(task, target_ns);
}

unsafe extern "C" fn cb_vsync(user_data: *mut c_void, baton: isize) {
    let s = shared(user_data);
    s.vsync.lock().unwrap().baton = Some(baton);
    s.runner.wake();
}

unsafe extern "C" fn cb_platform_message(message: *const ffi::FlutterPlatformMessage, user_data: *mut c_void) {
    let s = shared(user_data);
    let m = &*message;
    let channel = CStr::from_ptr(m.channel).to_string_lossy();
    let bytes = if m.message.is_null() { &[][..] } else { std::slice::from_raw_parts(m.message, m.message_size) };
    let (reply, effects) = s.handlers.lock().unwrap().on_message(&channel, bytes);
    if !m.response_handle.is_null() {
        (s.procs.SendPlatformMessageResponse.unwrap())(s.engine(), m.response_handle, reply.as_ptr(), reply.len());
    }
    s.apply(effects);
}

unsafe extern "C" fn cb_log(tag: *const c_char, message: *const c_char, _user_data: *mut c_void) {
    let tag = if tag.is_null() { "flutter".into() } else { CStr::from_ptr(tag).to_string_lossy() };
    let message = CStr::from_ptr(message).to_string_lossy();
    eprintln!("{tag}: {message}");
    crate::debug::on_log(&message);
}

unsafe extern "C" fn cb_resolve_locale(supported: *mut *const ffi::FlutterLocale, count: usize) -> *const ffi::FlutterLocale {
    // As GTK: the first supported locale.
    if count == 0 { std::ptr::null() } else { *supported }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales() {
        assert_eq!(locale_from_env(Some("de_AT.UTF-8")), ("de".into(), Some("AT".into())));
        assert_eq!(locale_from_env(Some("C")), ("en".into(), Some("US".into())));
        assert_eq!(locale_from_env(None), ("en".into(), Some("US".into())));
        assert_eq!(locale_from_env(Some("fr")), ("fr".into(), None));
    }
}
