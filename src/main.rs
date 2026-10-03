//! `aera-flutter`: started by `aera-plugin` (launcher/) with AERA's control
//! socket on fd 4 and the frame memfd on fd 3.
//!
//! Usage: `aera-flutter [--root DIR] [--vulkan | --gl] [--engine-arg ARG]...`,
//! or `aera-flutter --vulkan-info` to print what the Vulkan driver offers. The payload
//! root defaults to `AERA_PLUGIN_ROOT`, else two levels above this binary.
//! Developer engine switches also come from the environment and the plugin
//! data directory (`debug.rs`).

use std::os::fd::{FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::time::Duration;

use flutter_aera::engine::{Config, Engine};
use flutter_aera::host::{self, Control, Slots};

fn fd_from_env(name: &str, default: i32) -> i32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn main() {
    std::process::exit(run().unwrap_or_else(|e| {
        eprintln!("aera-flutter: {e}");
        2
    }));
}

fn run() -> Result<i32, String> {
    let mut root = std::env::var_os(host::ROOT_ENV).map(PathBuf::from);
    let mut engine_args = vec![];
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--root" => root = args.next().map(PathBuf::from),
            "--engine-arg" => engine_args.extend(args.next()),
            "--vulkan" | "--gl" => engine_args.push(a),
            // Diagnostics: what the Vulkan driver offers, then exit.
            "--vulkan-info" => {
                print!("{}", flutter_aera::renderer::vk::info()?);
                return Ok(0);
            }
            // Host API 2 passes --aera-host-api=N; the handshake decides.
            a if a.starts_with("--aera-host-api") => {}
            other => return Err(format!("unknown argument {other}")),
        }
    }
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

    // SAFETY: AERA hands us these descriptors open; we take sole ownership.
    let control = Control::plugin(unsafe { OwnedFd::from_raw_fd(fd_from_env(host::CONTROL_FD_ENV, host::CONTROL_FD)) });
    let surface = unsafe { OwnedFd::from_raw_fd(fd_from_env(host::SURFACE_FD_ENV, host::SURFACE_FD)) };
    let (session, early) = control.handshake(Duration::from_secs(10)).map_err(|e| format!("handshake: {e}"))?;
    let g = session.geometry;
    eprintln!("aera-flutter: surface {}x{} stride {} slots {} scale {} @{}Hz", g.width, g.height, g.stride, g.slots, g.scale, g.refresh_hz);
    let slots = Slots::map(surface, g).map_err(|e| format!("map surface: {e}"))?;

    let mut config = Config::from_root(&root);
    config.engine_args = engine_args;
    config.vulkan = vulkan;
    config.host_features = session.features;
    config.padding_dp = flutter_aera::padding::resolve(&root);
    let engine = Engine::start(config, control, slots)?;
    engine.replay(early);
    Ok(engine.run())
}
