//! `flutter/platform`, after GTK's `fl_platform_handler.cc`.
//!
//! Recovery has no system clipboard, so the clipboard is the process's own
//! (it lasts while the app runs). Haptics and system sounds are documented
//! no-ops (GTK's are too). Everything GTK leaves unhandled stays
//! not-implemented here.

use serde_json::{json, Value};

use super::codec::{self, decode_call};
use super::Effect;

pub const CHANNEL: &str = "flutter/platform";

#[derive(Default)]
pub struct Platform {
    clipboard: Option<String>,
}

impl Platform {
    pub fn handle(&mut self, bytes: &[u8], effects: &mut Vec<Effect>) -> Vec<u8> {
        let Some(call) = decode_call(bytes) else { return codec::error("Bad Arguments", "malformed method call") };
        match call.method.as_str() {
            "Clipboard.setData" => {
                self.clipboard = call.args["text"].as_str().map(str::to_owned);
                codec::success(Value::Null)
            }
            "Clipboard.getData" => match (&self.clipboard, call.args.as_str()) {
                (Some(text), Some("text/plain")) => codec::success(json!({ "text": text })),
                (_, Some("text/plain")) => codec::success(Value::Null),
                _ => codec::error("Unknown Clipboard Format", "only text/plain is supported"),
            },
            "Clipboard.hasStrings" => {
                codec::success(json!({ "value": self.clipboard.as_deref().is_some_and(|t| !t.is_empty()) }))
            }
            "HapticFeedback.vibrate" | "SystemSound.play" => codec::success(Value::Null),
            "SystemNavigator.pop" => {
                effects.push(Effect::Exit);
                codec::success(Value::Null)
            }
            // AERA can always close a plugin, so a cancelable request exits
            // too, as GTK does when nothing vetoes it.
            "System.exitApplication" => {
                effects.push(Effect::Exit);
                codec::success(json!({ "response": "exit" }))
            }
            "System.initializationComplete" => codec::success(Value::Null),
            _ => codec::not_implemented(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::codec::encode_call;

    #[test]
    fn clipboard_round_trip() {
        let mut p = Platform::default();
        let mut fx = vec![];
        assert_eq!(p.handle(&encode_call("Clipboard.hasStrings", json!("text/plain")), &mut fx), br#"[{"value":false}]"#);
        p.handle(&encode_call("Clipboard.setData", json!({ "text": "hi" })), &mut fx);
        assert_eq!(p.handle(&encode_call("Clipboard.getData", json!("text/plain")), &mut fx), br#"[{"text":"hi"}]"#);
        assert!(fx.is_empty());
    }

    #[test]
    fn pop_exits() {
        let mut fx = vec![];
        Platform::default().handle(&encode_call("SystemNavigator.pop", Value::Null), &mut fx);
        assert_eq!(fx, vec![Effect::Exit]);
    }

    #[test]
    fn unhandled_is_not_implemented() {
        let mut fx = vec![];
        let r = Platform::default().handle(&encode_call("SystemChrome.setPreferredOrientations", json!([])), &mut fx);
        assert!(r.is_empty());
    }
}
