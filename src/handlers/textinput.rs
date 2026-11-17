//! `flutter/textinput`, backed by `ime.rs` and desktop `PLAY_KEY` events.
//! Keyboard control requires the opt-in Host API 2 desktop integration feature.

use serde_json::{json, Value};

use super::codec::{self, decode_call, encode_call};
use super::Effect;
use crate::host::{feature, kind, Message};
use crate::ime::{Ime, KeyResult};

pub const CHANNEL: &str = "flutter/textinput";

pub fn handle(ime: &mut Ime, bytes: &[u8], features: u32, effects: &mut Vec<Effect>) -> Vec<u8> {
    let Some(call) = decode_call(bytes) else { return codec::error("Bad Arguments", "malformed method call") };
    match call.method.as_str() {
        "TextInput.setClient" => {
            let (Some(id), Some(config)) = (call.args[0].as_i64(), call.args.get(1)) else {
                return codec::error("Bad Arguments", "expected [client, configuration]");
            };
            ime.set_client(id, config);
        }
        "TextInput.clearClient" => ime.clear_client(),
        "TextInput.setEditingState" => ime.set_state(&call.args),
        "TextInput.show" | "TextInput.hide" => {
            if features & feature::DESKTOP_INTEGRATION == 0 {
                return codec::not_implemented();
            }
            let show = call.method == "TextInput.show";
            let flags = if !show { 0 } else if ime.digits() { 2 } else if ime.multiline() { 4 } else { 0 };
            effects.push(Effect::Host(Message::with(kind::SET_DESKTOP_KEYBOARD, 1, show as u32, flags)));
        }
        // Geometry and style hints only matter to a keyboard that floats by
        // the field; AERA's is docked. Documented no-ops, as GTK's.
        "TextInput.setEditableSizeAndTransform"
        | "TextInput.setMarkedTextRect"
        | "TextInput.setStyle"
        | "TextInput.setCaretRect"
        | "TextInput.requestAutofill"
        | "TextInput.finishAutofillContext" => {}
        _ => return codec::not_implemented(),
    }
    codec::success(Value::Null)
}

pub fn on_key(ime: &mut Ime, code: u32, effects: &mut Vec<Effect>) {
    let Some(client) = ime.client else { return };
    match ime.key(code) {
        KeyResult::Edited => effects.push(Effect::Send {
            channel: CHANNEL,
            bytes: encode_call("TextInputClient.updateEditingState", json!([client, ime.state()])),
        }),
        KeyResult::Action(action) => {
            effects.push(Effect::Send { channel: CHANNEL, bytes: encode_call("TextInputClient.performAction", json!([client, action])) })
        }
        KeyResult::Ignored => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_asks_aera_for_the_right_keyboard() {
        let mut ime = Ime::default();
        let mut fx = vec![];
        handle(&mut ime, &encode_call("TextInput.setClient", json!([3, { "inputType": { "name": "TextInputType.number" } }])), feature::DESKTOP_INTEGRATION, &mut fx);
        handle(&mut ime, &encode_call("TextInput.show", Value::Null), feature::DESKTOP_INTEGRATION, &mut fx);
        assert_eq!(fx, vec![Effect::Host(Message::with(kind::SET_DESKTOP_KEYBOARD, 1, 1, 2))]);
    }

    #[test]
    fn keys_update_the_framework() {
        let mut ime = Ime::default();
        let mut fx = vec![];
        handle(&mut ime, &encode_call("TextInput.setClient", json!([3, {}])), 0, &mut fx);
        on_key(&mut ime, 'x' as u32, &mut fx);
        let Effect::Send { bytes, .. } = &fx[0] else { panic!() };
        let call = decode_call(bytes).unwrap();
        assert_eq!(call.method, "TextInputClient.updateEditingState");
        assert_eq!(call.args[1]["text"], "x");
        fx.clear();
        on_key(&mut ime, 13, &mut fx);
        let Effect::Send { bytes, .. } = &fx[0] else { panic!() };
        assert_eq!(decode_call(bytes).unwrap().args, json!([3, "TextInputAction.done"]));
    }

    #[test]
    fn base_host_does_not_receive_keyboard_extensions() {
        let mut ime = Ime::default();
        let mut fx = vec![];
        assert!(handle(&mut ime, &encode_call("TextInput.show", Value::Null), 0, &mut fx).is_empty());
        assert!(handle(&mut ime, &encode_call("TextInput.hide", Value::Null), 0, &mut fx).is_empty());
        assert!(fx.is_empty());
    }
}
