//! Sound. Flutter has no audio system channel: apps play through a plugin
//! that speaks ALSA (miniaudio, so flutter_soloud; SDL; GStreamer's
//! alsasink). In the payload, ALSA's default device is AERA's audio bridge
//! (`audio/pcm_aera.c`, configured by `audio/aera.conf`).
//!
//! The bridge is AERA's own `/system/bin/aera-audio-bridge`, a closed
//! binary that only official AERA builds ship. Like AERA's Browser, Media,
//! Streams, Doom and RetroArch (`aeraui/features/*/launcher.cpp`), the
//! embedder starts it with `--browser-audio` when it starts, if it is
//! there, and stops it when it exits; the ALSA device connects to it on
//! each open. Without it, opening the device fails and apps play nothing.

use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command};
use std::time::Duration;

pub const BRIDGE: &str = "/system/bin/aera-audio-bridge";

/// The running bridge; stopped on drop.
pub struct Bridge(Child);

impl Bridge {
    /// Starts AERA's bridge if this AERA has one.
    pub fn start() -> Option<Bridge> {
        // SAFETY: access() on a NUL-terminated path.
        if unsafe { libc::access(c"/system/bin/aera-audio-bridge".as_ptr(), libc::X_OK) } != 0 {
            eprintln!("aera-flutter: no {BRIDGE}; no sound");
            return None;
        }
        let mut command = Command::new(Path::new(BRIDGE));
        command.arg0("aera-audio-bridge").arg("--browser-audio");
        // SAFETY: close_range is async-signal-safe. The bridge must not keep
        // AERA's control socket or our surface open after we exit.
        unsafe {
            command.pre_exec(|| {
                libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 0u32);
                Ok(())
            })
        };
        match command.spawn() {
            Ok(child) => Some(Bridge(child)),
            Err(e) => {
                eprintln!("aera-flutter: start {BRIDGE}: {e}");
                None
            }
        }
    }
}

impl Drop for Bridge {
    /// As AERA stops it: SIGTERM, half a second, then SIGKILL.
    fn drop(&mut self) {
        if let Ok(None) = self.0.try_wait() {
            // SAFETY: signalling our own child, not yet reaped.
            unsafe { libc::kill(self.0.id() as i32, libc::SIGTERM) };
            for _ in 0..25 {
                std::thread::sleep(Duration::from_millis(20));
                if !matches!(self.0.try_wait(), Ok(None)) {
                    return;
                }
            }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
