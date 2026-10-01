//! `flutter/lifecycle` from AERA's `LIFECYCLE`. The framework fills in the
//! states in between (`AppLifecycleListener`), so each host state maps to
//! one Flutter state, as GTK sends one per window change: resume (on
//! screen) `resumed`, inactive (a shade, sheet or AERA's picker over it)
//! `inactive`, pause (its scene left; AERA keeps the process and stops
//! returning frames until a new `SURFACE` and resume) `paused`, stop
//! `detached`.

use super::Effect;
use crate::host::lifecycle;

pub const CHANNEL: &str = "flutter/lifecycle";

pub fn message(state: &str) -> Effect {
    Effect::Send { channel: CHANNEL, bytes: format!("AppLifecycleState.{state}").into_bytes() }
}

pub fn from_host(value: u32) -> Option<Effect> {
    match value {
        lifecycle::RESUME => Some(message("resumed")),
        lifecycle::INACTIVE => Some(message("inactive")),
        lifecycle::PAUSE => Some(message("paused")),
        lifecycle::STOP => Some(message("detached")),
        _ => None,
    }
}
