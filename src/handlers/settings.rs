//! `flutter/settings`, sent once at start. Recovery has no settings service;
//! these are AERA's fixed look (light, 24-hour, unscaled text).

use serde_json::json;

use super::{codec, Effect};

pub const CHANNEL: &str = "flutter/settings";

pub fn initial() -> Effect {
    Effect::Send {
        channel: CHANNEL,
        bytes: codec::message(&json!({
            "textScaleFactor": 1.0,
            "alwaysUse24HourFormat": true,
            "platformBrightness": "light",
        })),
    }
}
