//! `aera-host-sim`: plays AERA's side of the generic pixel host on a PC.
//!
//! It makes the frame memfd and control socket as AERA would, starts the
//! embedder with them on fds 3 and 4, answers the handshake, takes each
//! `PRESENT` (answering `FRAME_DONE` on a simulated vsync), replays a
//! script of input, and writes frames as PNG files.
//!
//! ```text
//! aera-host-sim --root PAYLOAD [--embedder BIN] [--out DIR]
//!               [--size WxH] [--scale S] [--until MS] [--gpu]
//!               [--tap X,Y@MS]... [--type TEXT@MS]... [--back@MS]...
//!               [--snap NAME@MS]...
//! ```
//!
//! Times are milliseconds after the first frame. Positions are surface
//! pixels. `last.png` is always written at the end.
//!
//! It also runs on a device as a stand-in for AERA's host (built static, see
//! docs/device.md): `--gpu` then lets Mesa use the device's GPU instead of
//! forcing softpipe.

use std::collections::VecDeque;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Duration, Instant};

use flutter_aera::host::surface::create_memfd;
use flutter_aera::host::transport::pair;
use flutter_aera::host::{feature, kind, lifecycle, Control, Geometry, Message};

#[derive(Debug, Clone)]
enum Action {
    Tap(u32, u32),
    Type(String),
    Back,
    Snap(String),
}

struct Options {
    root: PathBuf,
    embedder: PathBuf,
    out: PathBuf,
    geometry: Geometry,
    until: Duration,
    /// Let Mesa pick the GPU (on a device) instead of forcing softpipe.
    gpu: bool,
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
        geometry: Geometry::PHONE,
        until: Duration::from_secs(5),
        gpu: false,
        script: vec![],
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--root" => o.root = value()?.into(),
            "--embedder" => o.embedder = value()?.into(),
            "--out" => o.out = value()?.into(),
            "--size" => {
                let v = value()?;
                let (w, h) = v.split_once('x').ok_or("--size WxH")?;
                o.geometry.width = w.parse().map_err(|_| "--size WxH")?;
                o.geometry.height = h.parse().map_err(|_| "--size WxH")?;
                o.geometry.stride = o.geometry.width * 4;
            }
            "--gpu" => o.gpu = true,
            "--scale" => o.geometry.scale = value()?.parse().map_err(|_| "--scale S")?,
            "--until" => o.until = Duration::from_millis(value()?.parse().map_err(|_| "--until MS")?),
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
            "--snap" => {
                let v = value()?;
                let (name, t) = at(&v)?;
                o.script.push((t, Action::Snap(name.to_owned())));
            }
            b if b.starts_with("--back@") => o.script.push((at(b)?.1, Action::Back)),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if o.root.as_os_str().is_empty() {
        return Err("--root PAYLOAD is required".into());
    }
    o.script.sort_by_key(|(t, _)| *t);
    Ok(o)
}

fn write_png(path: &Path, frame: &[u8], g: &Geometry) -> Result<(), String> {
    let mut rgba = Vec::with_capacity(g.width as usize * g.height as usize * 4);
    for row in frame.chunks(g.stride as usize).take(g.height as usize) {
        for px in row[..g.width as usize * 4].chunks_exact(4) {
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), g.width, g.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().and_then(|mut w| w.write_image_data(&rgba)).map_err(|e| e.to_string())?;
    println!("sim: wrote {}", path.display());
    Ok(())
}

/// Moves `from` to fd `to` in the child, clearing close-on-exec.
fn place(from: i32, to: i32) -> std::io::Result<()> {
    // SAFETY: async-signal-safe calls between fork and exec.
    unsafe {
        if from == to {
            let flags = libc::fcntl(to, libc::F_GETFD);
            libc::fcntl(to, libc::F_SETFD, flags & !libc::FD_CLOEXEC);
        } else if libc::dup2(from, to) < 0 {
            return Err(std::io::Error::last_os_error());
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

fn run() -> Result<ExitCode, String> {
    let o = parse()?;
    let g = o.geometry;
    std::fs::create_dir_all(&o.out).map_err(|e| e.to_string())?;
    let data = o.out.join("plugin-data");
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;

    let memfd: OwnedFd = create_memfd(&g).map_err(|e| e.to_string())?;
    let (plugin_end, host_end) = pair().map_err(|e| e.to_string())?;
    // SAFETY: the host's own mapping of the memfd it created.
    let base = unsafe {
        libc::mmap(std::ptr::null_mut(), g.total_bytes(), libc::PROT_READ, libc::MAP_SHARED, memfd.as_raw_fd(), 0)
    };
    if base == libc::MAP_FAILED {
        return Err("mmap surface".into());
    }
    // SAFETY: the mapping lives until the process exits.
    let surface = unsafe { std::slice::from_raw_parts(base as *const u8, g.total_bytes()) };

    let (m, p) = (memfd.as_raw_fd(), plugin_end.as_raw_fd());
    let mut command = Command::new(&o.embedder);
    command
        .arg("--aera-host-api=3")
        .env("AERA_PLUGIN_ROOT", &o.root)
        .env("AERA_PLUGIN_DATA", std::fs::canonicalize(&data).map_err(|e| e.to_string())?)
        .env("AERA_PLUGIN_FD", "4")
        .env("AERA_SURFACE_FD", "3");
    // Mesa 25's llvmpipe crashes (a JIT call through a null pointer) once
    // libflutter_engine.so is loaded; softpipe draws the same frames.
    // On a device, --gpu leaves the choice to the launcher and Mesa.
    if !o.gpu && std::env::var_os("GALLIUM_DRIVER").is_none() {
        command.env("GALLIUM_DRIVER", "softpipe");
    }
    // SAFETY: only dup2/fcntl run in the child before exec. The order avoids
    // clobbering: park both above 10 first, then place them.
    unsafe {
        command.pre_exec(move || {
            let mm = libc::fcntl(m, libc::F_DUPFD, 10);
            let pp = libc::fcntl(p, libc::F_DUPFD, 10);
            place(mm, 3)?;
            place(pp, 4)?;
            Ok(())
        });
    }
    let mut child = command.spawn().map_err(|e| format!("start {}: {e}", o.embedder.display()))?;
    drop(plugin_end);
    let host = Control::host(host_end);

    let hello = host.recv(Some(Duration::from_secs(10))).map_err(|e| e.to_string())?.ok_or("no HELLO")?;
    if hello.kind != kind::HELLO || !(hello.value..=hello.flags).contains(&3) {
        return Err(format!("bad HELLO {hello:?}"));
    }
    let features = feature::PIXEL_SURFACE | feature::BACK_NAVIGATION | feature::KEYBOARD_INSET;
    host.send(&Message::with(kind::HELLO_ACK, 0, features, 0)).map_err(|e| e.to_string())?;
    host.send(&g.to_message()).map_err(|e| e.to_string())?;
    host.send(&Message::with(kind::LIFECYCLE, 0, lifecycle::RESUME, 0)).map_err(|e| e.to_string())?;
    println!("sim: handshake done, {}x{} x{} slots", g.width, g.height, g.slots);

    let period = g.frame_period();
    let started = Instant::now();
    let mut first_frame: Option<Instant> = None;
    let mut script: VecDeque<(Duration, Action)> = o.script.into();
    let mut last: Option<Vec<u8>> = None;
    // Frames shown but not yet released: (sequence, release time).
    let mut showing: VecDeque<(u32, Instant)> = VecDeque::new();
    let mut frames = 0u32;
    let mut closed = false;
    let mut keyboard = false;
    let mut next_vsync = Instant::now() + period;

    loop {
        let now = Instant::now();
        if now >= next_vsync {
            // Like a compositor: a presented frame is released at the vsync
            // after it was shown.
            while showing.front().is_some_and(|(_, t)| *t <= now) {
                let (seq, _) = showing.pop_front().unwrap();
                let _ = host.send(&Message::with(kind::FRAME_DONE, seq, 0, 0));
            }
            next_vsync += period * ((now - next_vsync).as_nanos() / period.as_nanos() + 1) as u32;
        }
        if let Some(t0) = first_frame {
            let since = now - t0;
            while script.front().is_some_and(|(t, _)| *t <= since) {
                let (t, action) = script.pop_front().unwrap();
                println!("sim: {:>6} ms {action:?}", t.as_millis());
                match action {
                    Action::Tap(x, y) => {
                        let _ = host.send(&Message::with(kind::TOUCH_DOWN, 0, x, y));
                        let _ = host.send(&Message::with(kind::TOUCH_UP, 0, x, y));
                    }
                    Action::Type(text) => {
                        for c in text.chars() {
                            let code = match c { '\u{8}' => 8, '\n' => 13, c => c as u32 };
                            let _ = host.send(&Message::with(kind::KEY, 0, code, 0));
                        }
                    }
                    Action::Back => {
                        let _ = host.send(&Message::new(kind::BACK));
                    }
                    Action::Snap(name) => match &last {
                        Some(f) => write_png(&o.out.join(format!("{name}.png")), f, &g)?,
                        None => println!("sim: no frame yet for {name}"),
                    },
                }
            }
            if since >= o.until {
                break;
            }
        } else if now - started > Duration::from_secs(60) {
            return Err("no frame within 60 s".into());
        }
        if closed {
            break;
        }
        let wait = next_vsync.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
        match host.recv(Some(wait)) {
            Ok(Some(msg)) => match msg.kind {
                kind::PRESENT => {
                    let slot = msg.value as usize;
                    if slot >= g.slots as usize {
                        return Err(format!("PRESENT for slot {slot}"));
                    }
                    let frame = &surface[slot * g.frame_bytes()..][..g.frame_bytes()];
                    last = Some(frame.to_vec());
                    frames += 1;
                    if first_frame.is_none() {
                        first_frame = Some(Instant::now());
                        println!("sim: first frame after {} ms", started.elapsed().as_millis());
                    }
                    showing.push_back((msg.request_id, next_vsync));
                }
                kind::KEYBOARD_SHOW => {
                    println!("sim: keyboard show (purpose {}, multiline {})", msg.value, msg.flags);
                    if !keyboard {
                        keyboard = true;
                        let _ = host.send(&Message::with(kind::KEYBOARD_INSET, 0, g.height * 2 / 5, 0));
                    }
                }
                kind::KEYBOARD_HIDE => {
                    println!("sim: keyboard hide");
                    keyboard = false;
                    let _ = host.send(&Message::with(kind::KEYBOARD_INSET, 0, 0, 0));
                }
                kind::SET_STATUS => println!("sim: status {:?}", msg.text),
                kind::CLOSE => {
                    println!("sim: plugin asked to close");
                    closed = true;
                }
                other => println!("sim: ignoring kind {other}"),
            },
            Ok(None) => {}
            Err(e) => {
                println!("sim: control socket closed ({e})");
                closed = true;
            }
        }
    }
    println!("sim: {frames} frames");
    if let Some(f) = &last {
        write_png(&o.out.join("last.png"), f, &g)?;
    }
    if !closed {
        let _ = host.send(&Message::with(kind::LIFECYCLE, 0, lifecycle::STOP, 0));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                println!("sim: embedder exited with {status}");
                break;
            }
            _ if Instant::now() > deadline => {
                let _ = child.kill();
                println!("sim: embedder killed after 5 s");
                break;
            }
            _ => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    drop(host);
    Ok(if frames > 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}
