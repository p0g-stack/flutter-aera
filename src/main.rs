//! `aera-flutter`: started by `aera-plugin` (launcher/) as an upstream Host
//! API 2 worker: the control socket on fd 4, `AERA_HOST_API=2`,
//! `--aera-host-api=2`, and nothing else inherited.
//!
//! Usage: `aera-flutter [--root DIR] [--vulkan | --gl] [--desktop WxH]
//! [--engine-arg ARG]...`, or `aera-flutter --vulkan-info` to print what the
//! Vulkan driver offers. `--desktop` picks one of AERA's desktop sizes
//! (default AERA's 960x540). The payload root defaults to
//! `AERA_PLUGIN_ROOT`, else two levels above this binary. Developer engine
//! switches also come from the environment and the plugin data directory
//! (`debug.rs`).

use std::os::fd::{AsFd, FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::time::Duration;

use flutter_aera::engine::{Config, Engine};
use flutter_aera::host::desktop::{self, Request};
use flutter_aera::host::{self, Control};

/// The setup page AERA shows behind the desktop.
const TITLE: &str = "Flutter";

fn main() {
    std::process::exit(run().unwrap_or_else(|e| {
        eprintln!("aera-flutter: {e}");
        2
    }));
}

fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let bad = || format!("--desktop {text}: expected WIDTHxHEIGHT");
    let (w, h) = text.split_once('x').ok_or_else(bad)?;
    let size = (w.parse().map_err(|_| bad())?, h.parse().map_err(|_| bad())?);
    if !host::DESKTOP_SIZES.contains(&size) {
        return Err(format!("--desktop {text}: AERA offers {:?}", host::DESKTOP_SIZES));
    }
    Ok(size)
}

/// Upstream's launch contract: anything else is not Host API 2.
fn check_launch() -> Result<(), String> {
    match std::env::var(host::HOST_API_ENV) {
        Ok(v) if v == host::HOST_API => {}
        Ok(v) => return Err(format!("{}={v}; this embedder speaks Host API 2", host::HOST_API_ENV)),
        Err(_) => return Err(format!("{} is not set; start under AERA Host API 2", host::HOST_API_ENV)),
    }
    if let Ok(v) = std::env::var(host::CONTROL_FD_ENV) {
        if v != host::CONTROL_FD.to_string() {
            return Err(format!("{}={v}; Host API 2 uses fd {}", host::CONTROL_FD_ENV, host::CONTROL_FD));
        }
    }
    Ok(())
}

fn run() -> Result<i32, String> {
    let mut root = std::env::var_os(host::ROOT_ENV).map(PathBuf::from);
    let mut engine_args = vec![];
    let mut desktop_size = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--root" => root = args.next().map(PathBuf::from),
            "--engine-arg" => engine_args.extend(args.next()),
            "--vulkan" | "--gl" => engine_args.push(a),
            "--desktop" => desktop_size = Some(parse_size(&args.next().unwrap_or_default())?),
            // Diagnostics: what the Vulkan driver offers, then exit.
            "--vulkan-info" => {
                print!("{}", flutter_aera::renderer::vk::info()?);
                return Ok(0);
            }
            host::HOST_API_ARG => {}
            a if a.starts_with("--aera-host-api") => return Err(format!("{a}: this embedder speaks Host API 2")),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    check_launch()?;
    let root = match root {
        Some(r) => r,
        None => {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            exe.ancestors().nth(3).ok_or("cannot find the payload root")?.to_path_buf()
        }
    };
    flutter_aera::env::apply(&root);
    // Stopped when run() returns.
    let _bridge = flutter_aera::audio::Bridge::start();
    flutter_aera::debug::clear_url();
    engine_args.extend(flutter_aera::debug::switches());
    let vulkan = flutter_aera::renderer::vk::take_switch(&mut engine_args);
    if !engine_args.is_empty() {
        eprintln!("aera-flutter: engine switches {engine_args:?}");
    }

    // SAFETY: AERA hands us fd 4 open; we take sole ownership.
    let control = unsafe { OwnedFd::from_raw_fd(host::CONTROL_FD) };
    host::play::check_seqpacket(control.as_fd()).map_err(|e| format!("fd {}: {e}", host::CONTROL_FD))?;
    let control = Control::plugin(control);
    let (session, mut early) =
        control.handshake(host::REQUESTED_FEATURES, Duration::from_secs(10)).map_err(|e| format!("handshake: {e}"))?;
    eprintln!("aera-flutter: host features {:#x}", session.features);
    let send = |m: &host::Message| control.send(m).map_err(|e| format!("send to host: {e}"));
    for m in desktop::setup_page(TITLE) {
        send(&m)?;
    }
    let request = Request::new(1, desktop_size, session.features)?;
    let (desktop, more) = match desktop::open(&control, &request, Duration::from_secs(10)) {
        Ok(opened) => opened,
        Err(e) => {
            let _ = send(&desktop::status(&format!("Desktop unavailable: {e}")));
            return Err(format!("desktop: {e}"));
        }
    };
    early.extend(more);
    let g = desktop.geometry();
    eprintln!("aera-flutter: desktop {}x{} {} slots", g.width, g.height, g.slots);

    let mut config = Config::from_root(&root);
    config.engine_args = engine_args;
    config.vulkan = vulkan;
    config.host_features = session.features;
    config.desktop_size = desktop_size;
    config.padding_dp = flutter_aera::padding::resolve(&root);
    let engine = Engine::start(config, control, desktop)?;
    engine.replay(early);
    Ok(engine.run())
}
