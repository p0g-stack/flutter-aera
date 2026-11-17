//! `aera-host-sim`: plays AERA's side of Host API 2 on a PC.
//!
//! It starts the embedder as AERA's plugin launcher does (the control
//! socket on fd 4 only, `AERA_HOST_API=2`, `--aera-host-api=2`), answers
//! the handshake and the setup page, and when the plugin asks with
//! `OPEN_DESKTOP` hands over a sealed two-slot BGRA memfd and the input
//! channel in `DESKTOP_READY`'s `SCM_RIGHTS`. Then it runs AERA's 16 ms
//! desktop timer (ACK the frame received last tick, then read the next),
//! replays a script of touches and keys as AERA's toolbar, game controls
//! and keyboard turn them into `PlayMessage`s, and writes frames as PNG.
//! The recovery extensions (desktop integration, desktop text input) are
//! granted when asked unless `--base-api2` is given.
//!
//! ```text
//! aera-host-sim --root PAYLOAD [--embedder BIN] [--out DIR] [--until MS]
//!               [--gpu] [--base-api2] [--pick-file ABSPATH] [--edit-text TEXT]
//!               [--tap X,Y@MS]... [--type TEXT@MS]... [--back@MS]...
//!               [--action ID@MS]... [--snap NAME@MS]...
//! ```
//!
//! Times are milliseconds after the first desktop frame; `--until 0` runs
//! until the embedder exits. `--tap` positions are desktop pixels (the
//! simulated panel is the desktop frame, scale 1); taps in AERA's toolbar
//! strip (virtual 960x540 layout: x < 580, y < 65) work its buttons, EXIT
//! included. `--type` presses AERA's on-screen keyboard, which must be up
//! (the KEYS button, or the app's `SET_DESKTOP_KEYBOARD`); `\n` is its OK
//! key unless the app asked for a multiline keyboard. `--back` cancels an
//! open picker or editor, else taps ESC on the desktop, else runs the
//! page's back action; `--action` presses a setup-page button.
//! `--pick-file` is the file or folder the simulated user picks
//! (`MODAL_ANSWER_MS` after the picker opens); `--edit-text` is what they
//! submit in AERA's native editor. Without them the modal stays open until
//! `--back`.
//!
//! `last.png` is always written at the end. `OUT/plugin-data` is passed as
//! `AERA_PLUGIN_DATA` (a PC convenience; AERA does not set it); its
//! `vm-service-url` is what `flutter attach --debug-url` takes. While it
//! runs, a file `OUT/snap` holding NAME writes NAME.png and `OUT/stop` ends
//! the run.
//!
//! It also runs on a device as a stand-in for AERA's host (built static, see
//! docs/device.md): `--gpu` then lets Mesa use the device's GPU instead of
//! forcing softpipe.

mod clipboard;
mod desktop;
mod session;

use std::collections::VecDeque;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use flutter_aera::host::surface::create_memfd_of;
use flutter_aera::host::transport::pair;
use flutter_aera::host::{
    kind, Control, Message, PlayChannel, CONTROL_FD, CONTROL_FD_ENV, DATA_ENV, HOST_API, HOST_API_ARG,
    HOST_API_ENV, ROOT_ENV,
};

use desktop::Tick;
use session::{Config, Out, Session};

/// AERA's desktop timer period (`lv_timer_create(..., 16, ...)`).
const TICK: Duration = Duration::from_millis(16);

#[derive(Debug, Clone)]
enum Action {
    Tap(i32, i32),
    Type(String),
    Back,
    Press(u32),
    Snap(String),
}

struct Options {
    root: PathBuf,
    embedder: PathBuf,
    out: PathBuf,
    until: Duration,
    /// Let Mesa pick the GPU (on a device) instead of forcing softpipe.
    gpu: bool,
    base_only: bool,
    pick_file: Option<PathBuf>,
    edit_text: Option<String>,
    script: Vec<(Duration, Action)>,
}

fn at(value: &str) -> Result<(&str, Duration), String> {
    let (what, ms) = value.rsplit_once('@').ok_or(format!("{value}: expected …@MS"))?;
    Ok((what, Duration::from_millis(ms.parse().map_err(|_| format!("{value}: bad time"))?)))
}

fn parse() -> Result<Options, String> {
    let here = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut o = Options {
        root: PathBuf::new(),
        embedder: here.with_file_name("aera-flutter"),
        out: PathBuf::from("out/sim"),
        until: Duration::from_secs(5),
        gpu: false,
        base_only: false,
        pick_file: None,
        edit_text: None,
        script: vec![],
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--root" => o.root = value()?.into(),
            "--embedder" => o.embedder = value()?.into(),
            "--out" => o.out = value()?.into(),
            "--gpu" => o.gpu = true,
            "--base-api2" => o.base_only = true,
            "--until" => o.until = Duration::from_millis(value()?.parse().map_err(|_| "--until MS")?),
            "--pick-file" => {
                let path = PathBuf::from(value()?);
                if !path.is_absolute() {
                    return Err("--pick-file takes an absolute path".into());
                }
                std::fs::metadata(&path).map_err(|e| format!("--pick-file {}: {e}", path.display()))?;
                o.pick_file = Some(path);
            }
            "--edit-text" => o.edit_text = Some(value()?),
            "--tap" => {
                let v = value()?;
                let (xy, t) = at(&v)?;
                let (x, y) = xy.split_once(',').ok_or("--tap X,Y@MS")?;
                o.script.push((t, Action::Tap(x.parse().map_err(|_| "x")?, y.parse().map_err(|_| "y")?)));
            }
            "--type" => {
                let v = value()?;
                let (text, t) = at(&v)?;
                o.script.push((t, Action::Type(text.to_owned())));
            }
            "--action" => {
                let v = value()?;
                let (id, t) = at(&v)?;
                o.script.push((t, Action::Press(id.parse().map_err(|_| "--action ID@MS")?)));
            }
            "--snap" => {
                let v = value()?;
                let (name, t) = at(&v)?;
                o.script.push((t, Action::Snap(name.to_owned())));
            }
            b if b.starts_with("--back@") => o.script.push((at(b)?.1, Action::Back)),
            "--size" | "--bar" | "--scale" => {
                return Err(format!("{a}: Host API 2's desktop size is the plugin's OPEN_DESKTOP request"));
            }
            r if r.starts_with("--rotate@") => return Err("--rotate: Host API 2's desktop is fixed landscape".into()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if o.root.as_os_str().is_empty() {
        return Err("--root PAYLOAD is required".into());
    }
    o.script.sort_by_key(|(t, _)| *t);
    Ok(o)
}

/// A copy of one desktop frame.
struct Shot {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

fn write_png(path: &Path, shot: &Shot) -> Result<(), String> {
    let mut rgba = Vec::with_capacity(shot.pixels.len());
    for px in shot.pixels.chunks_exact(4) {
        rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
    }
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(io::BufWriter::new(file), shot.width, shot.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().and_then(|mut w| w.write_image_data(&rgba)).map_err(|e| e.to_string())?;
    println!("sim: wrote {}", path.display());
    Ok(())
}

/// The host's half of an open desktop: the frame memfd it allocated and
/// maps read-only, and its end of the input channel.
struct Surface {
    play: PlayChannel,
    _memfd: OwnedFd,
    base: *const u8,
    len: usize,
}

impl Surface {
    /// `OpenPluginDesktop` + `Session::SendDesktop`.
    fn open(control: &Control, request_id: u32, width: u32, height: u32) -> io::Result<Surface> {
        let len = 2 * width as usize * height as usize * 4;
        let (ours, theirs) = pair()?;
        let play = PlayChannel::new(ours)?;
        let memfd = create_memfd_of(len)?;
        // SAFETY: a fresh read-only shared mapping of a memfd sealed at `len`.
        let base = unsafe {
            libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ, libc::MAP_SHARED, memfd.as_raw_fd(), 0)
        };
        if base == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        // From here `Drop` unmaps.
        let s = Surface { play, _memfd: memfd, base: base as *const u8, len };
        let ready = Message::with(kind::DESKTOP_READY, request_id, width, height);
        control.send_with_fds(&ready, &[s._memfd.as_raw_fd(), theirs.as_raw_fd()])?;
        Ok(s)
    }

    fn shot(&self, slot: usize, width: u32, height: u32) -> Shot {
        let bytes = width as usize * height as usize * 4;
        // SAFETY: the mapping is `len` bytes, two slots of `bytes`, alive
        // while `self` is.
        let all = unsafe { std::slice::from_raw_parts(self.base, self.len) };
        Shot { width, height, pixels: all[slot * bytes..][..bytes].to_vec() }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: the mapping made in `open`; no slice of it outlives `self`.
        unsafe { libc::munmap(self.base as *mut libc::c_void, self.len) };
    }
}

/// Moves `from` to fd `to` in the child, clearing close-on-exec.
fn place(from: i32, to: i32) -> io::Result<()> {
    // SAFETY: async-signal-safe calls between fork and exec.
    unsafe {
        if from == to {
            let flags = libc::fcntl(to, libc::F_GETFD);
            libc::fcntl(to, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
        } else if libc::dup2(from, to) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("sim: {e}");
            ExitCode::from(2)
        }
    }
}

/// The I/O around one [`Session`].
struct Host {
    session: Session,
    control: Option<Control>,
    surface: Option<Surface>,
    /// The frame on screen when the desktop last closed.
    last: Option<Shot>,
}

impl Host {
    fn send_play(&mut self) {
        let (Some(d), Some(s)) = (&mut self.session.desktop, &self.surface) else { return };
        for p in std::mem::take(&mut d.out) {
            if !d.connected() {
                break;
            }
            if let Err(e) = s.play.send(&p) {
                println!("sim: input channel send failed ({e}); Application stopped");
                d.disconnect();
            }
        }
    }

    /// The frame on screen now, or the last one shown.
    fn shot(&self) -> Option<Shot> {
        match (&self.session.desktop, &self.surface) {
            (Some(d), Some(s)) => d.latched_slot().map(|slot| s.shot(slot, d.width, d.height)),
            _ => self.last.as_ref().map(|l| Shot { pixels: l.pixels.clone(), ..*l }),
        }
    }

    fn snap(&self, dir: &Path, name: &str) -> Result<(), String> {
        match self.shot() {
            Some(shot) => write_png(&dir.join(format!("{name}.png")), &shot),
            None => {
                println!("sim: no frame yet for {name}");
                Ok(())
            }
        }
    }

    /// Carries out everything the session queued.
    fn flush(&mut self) {
        loop {
            self.send_play();
            if self.session.out.is_empty() {
                return;
            }
            for o in std::mem::take(&mut self.session.out) {
                match o {
                    Out::Control(m) => {
                        if let Some(c) = &self.control {
                            if let Err(e) = c.send(&m) {
                                println!("sim: control send failed ({e})");
                                self.session.peer_gone();
                                self.control = None;
                            }
                        }
                    }
                    Out::OpenDesktop { request_id, width, height } => {
                        let Some(c) = &self.control else { continue };
                        match Surface::open(c, request_id, width, height) {
                            Ok(s) => {
                                self.surface = Some(s);
                                self.session.desktop_opened(width, height);
                            }
                            Err(e) => {
                                println!("sim: could not open the desktop ({e})");
                                self.session.desktop_failed(request_id);
                            }
                        }
                    }
                    Out::CloseDesktop => {
                        self.send_play();
                        if let Some(shot) = self.shot() {
                            self.last = Some(shot);
                        }
                        self.session.desktop = None;
                        self.surface = None;
                        println!("sim: desktop closed");
                    }
                    Out::Disconnect => self.control = None,
                }
            }
        }
    }
}

fn run() -> Result<ExitCode, String> {
    let o = parse()?;
    std::fs::create_dir_all(&o.out).map_err(|e| e.to_string())?;
    let data = o.out.join("plugin-data");
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let root = std::fs::canonicalize(&o.root).map_err(|e| format!("{}: {e}", o.root.display()))?;

    let (plugin_end, host_end) = pair().map_err(|e| e.to_string())?;
    let p = plugin_end.as_raw_fd();
    let mut command = Command::new(&o.embedder);
    command
        .arg(HOST_API_ARG)
        .current_dir(&root)
        .env("HOME", "/tmp")
        .env("TMPDIR", "/tmp")
        .env("LANG", "en.UTF-8")
        .env("AERA_LOCALE", "en")
        .env(CONTROL_FD_ENV, CONTROL_FD.to_string())
        .env(HOST_API_ENV, HOST_API)
        .env(ROOT_ENV, &root)
        .env("XDG_DATA_DIRS", root.join("usr/share"))
        .env(DATA_ENV, std::fs::canonicalize(&data).map_err(|e| e.to_string())?)
        .process_group(0);
    // Mesa 25's llvmpipe crashes (a JIT call through a null pointer) once
    // libflutter_engine.so is loaded; softpipe draws the same frames.
    // On a device, --gpu leaves the choice to the launcher and Mesa.
    if !o.gpu && std::env::var_os("GALLIUM_DRIVER").is_none() {
        command.env("GALLIUM_DRIVER", "softpipe");
    }
    // SAFETY: only fcntl/close/dup2 run in the child before exec. As AERA's
    // launcher does: nothing on fd 3, the control socket on fd 4. Parking
    // it above 10 first keeps `close(3)` from closing it.
    unsafe {
        command.pre_exec(move || {
            let parked = libc::fcntl(p, libc::F_DUPFD_CLOEXEC, 10);
            if parked < 0 {
                return Err(io::Error::last_os_error());
            }
            libc::close(3);
            place(parked, CONTROL_FD)
        });
    }
    let mut child = command.spawn().map_err(|e| format!("start {}: {e}", o.embedder.display()))?;
    drop(plugin_end);

    let mut config = Config::new(o.base_only);
    config.pick_file = o.pick_file.clone();
    config.edit_text = o.edit_text.clone();
    let mut host = Host { session: Session::new(config), control: Some(Control::host(host_end)), surface: None, last: None };
    println!("sim: Host API 2{}", if o.base_only { ", base only" } else { " with recovery extensions" });

    let started = Instant::now();
    let ms = |t: Instant| t.duration_since(started).as_millis() as u64;
    let mut next_tick = started + TICK;
    let mut first_frame: Option<Instant> = None;
    let mut script: VecDeque<(Duration, Action)> = o.script.into();
    let mut frames = 0u32;
    let mut play_fault: Option<String> = None;

    loop {
        let now = Instant::now();
        if now < next_tick {
            let wait = next_tick - now;
            match &host.control {
                Some(c) => match c.recv(Some(wait)) {
                    Ok(Some(m)) => host.session.receive(&m, ms(Instant::now())),
                    Ok(None) => {}
                    // `Session::Poll`: a malformed or wrong-way packet fails
                    // the session and AERA closes the socket.
                    Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                        host.session.fail(format!("The plugin closed or sent an invalid message ({e})."));
                    }
                    Err(e) => {
                        println!("sim: control socket closed ({e})");
                        host.session.peer_gone();
                        host.control = None;
                    }
                },
                None => std::thread::sleep(wait),
            }
            host.flush();
            continue;
        }
        next_tick += TICK;
        if next_tick <= now {
            next_tick = now + TICK;
        }
        let t = ms(now);
        host.session.poll(t);

        if let Some(t0) = first_frame {
            let since = now - t0;
            while script.front().is_some_and(|(at, _)| *at <= since) {
                let (at, action) = script.pop_front().unwrap();
                println!("sim: {:>6} ms {action:?}", at.as_millis());
                match action {
                    Action::Tap(x, y) => host.session.tap(x, y, t),
                    Action::Type(text) => host.session.type_text(&text),
                    Action::Back => host.session.back(t),
                    Action::Press(id) => host.session.action(id),
                    Action::Snap(name) => host.snap(&o.out, &name)?,
                }
            }
            if !o.until.is_zero() && since >= o.until {
                break;
            }
        } else if now - started > Duration::from_secs(60) {
            return Err("no desktop frame within 60 s".into());
        }

        // AERA's desktop timer.
        if let Some(d) = &mut host.session.desktop {
            match d.tick() {
                Tick::Close => host.session.close_desktop(),
                Tick::Idle => {}
                Tick::Receive => {
                    // The ACK this tick queued goes out before the read.
                    host.send_play();
                    let (Some(d), Some(s)) = (&mut host.session.desktop, &host.surface) else { unreachable!() };
                    match s.play.try_recv() {
                        Ok(Some(m)) => match d.frame(m) {
                            Ok(_) => {
                                frames += 1;
                                if first_frame.is_none() {
                                    first_frame = Some(now);
                                    println!("sim: first frame after {} ms", t);
                                }
                            }
                            Err(why) => {
                                println!("sim: bad input-channel message: {why}; Application stopped");
                                play_fault = Some(why);
                            }
                        },
                        Ok(None) => {}
                        Err(e) => {
                            println!("sim: input channel closed ({e}); Application stopped");
                            if e.kind() == io::ErrorKind::InvalidData {
                                play_fault = Some(e.to_string());
                            }
                            d.disconnect();
                        }
                    }
                }
            }
        }
        host.flush();

        // Script hooks for interactive runs (ci/hot-reload-check.sh):
        // OUT/snap holding NAME writes NAME.png; OUT/stop ends the run.
        // A writer should rename the file into place; an empty read is a
        // write still in progress, so it is left for the next turn.
        if let Some(name) = std::fs::read_to_string(o.out.join("snap")).ok().filter(|n| !n.trim().is_empty()) {
            let _ = std::fs::remove_file(o.out.join("snap"));
            host.snap(&o.out, name.trim())?;
        }
        if o.out.join("stop").exists() {
            let _ = std::fs::remove_file(o.out.join("stop"));
            break;
        }
        if let Ok(Some(status)) = child.try_wait() {
            println!("sim: embedder exited with {status}");
            break;
        }
    }

    println!("sim: {frames} frames");
    if let Some(shot) = host.shot() {
        write_png(&o.out.join("last.png"), &shot)?;
    }
    // The scene goes away: the desktop first, then LIFECYCLE STOP.
    host.session.close_desktop();
    host.session.stop();
    host.flush();
    host.control = None;
    let deadline = Instant::now() + Duration::from_secs(5);
    let child_ok = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                println!("sim: embedder exited with {status}");
                break status.success();
            }
            _ if Instant::now() > deadline => {
                let _ = child.kill();
                println!("sim: embedder killed after 5 s");
                let _ = child.wait();
                break false;
            }
            _ => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    if let Some(reason) = &host.session.failed {
        println!("sim: FAIL: AERA rejected the plugin: {reason}");
    }
    if let Some(why) = &play_fault {
        println!("sim: FAIL: input channel protocol: {why}");
    }
    let ok = frames > 0 && child_ok && host.session.failed.is_none() && play_fault.is_none();
    Ok(if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
