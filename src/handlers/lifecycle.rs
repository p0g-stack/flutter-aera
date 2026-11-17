//! `flutter/lifecycle` from AERA's `LIFECYCLE`. The framework fills in the
//! states in between (`AppLifecycleListener`), so each host state maps to
//! one Flutter state: resume → `resumed`, pause → `paused`, stop → `detached`.
//! Upstream desktop plugins cannot opt into background execution; leaving
//! the scene stops the process. Do not invent the old API 3 inactive state.

use super::Effect;
use crate::host::lifecycle;

pub const CHANNEL: &str = "flutter/lifecycle";

pub fn message(state: &str) -> Effect {
    Effect::Send { channel: CHANNEL, bytes: format!("AppLifecycleState.{state}").into_bytes() }
}

pub fn from_host(value: u32) -> Option<Effect> {
    match value {
        lifecycle::RESUME => Some(message("resumed")),
        lifecycle::PAUSE => Some(message("paused")),
        lifecycle::STOP => Some(message("detached")),
        _ => None,
    }
}
