//! `flutter/lifecycle` from Host API 2's `LIFECYCLE`. The framework fills in
//! the states in between (`AppLifecycleListener`), so each host state maps to
//! one Flutter state, as GTK sends one per window change.

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
