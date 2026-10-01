//! `flutter/settings`, sent once at start. GTK reads the desktop's colour
//! scheme; here it is AERA's light or dark surface (`AERA_APPEARANCE`, Host
//! API 3), light if unset. The clock format is AERA's saved
//! `tw_military_time` ([`crate::aera_settings`]); text is not scaled for apps.

use serde_json::json;

use super::{codec, Effect};

pub const CHANNEL: &str = "flutter/settings";

pub fn brightness(appearance: Option<&str>) -> &'static str {
    if appearance == Some("dark") { "dark" } else { "light" }
}

pub fn initial(settings: &crate::aera_settings::Settings) -> Effect {
    let appearance = std::env::var("AERA_APPEARANCE").ok();
    Effect::Send {
        channel: CHANNEL,
        bytes: codec::message(&json!({
            "textScaleFactor": 1.0,
            "alwaysUse24HourFormat": settings.clock_24h(),
            "platformBrightness": brightness(appearance.as_deref()),
        })),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn brightness_follows_aera() {
        assert_eq!(super::brightness(Some("dark")), "dark");
        assert_eq!(super::brightness(Some("light")), "light");
        assert_eq!(super::brightness(None), "light");
    }
}
