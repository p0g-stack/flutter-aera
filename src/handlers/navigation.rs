//! `flutter/navigation`: AERA's Back gesture becomes `popRoute`, as Android's
//! back does. When nothing is left to pop, the framework answers with
//! `SystemNavigator.pop`, which `platform` turns into `CLOSE`.

use serde_json::Value;

use super::codec::encode_call;
use super::Effect;

pub const CHANNEL: &str = "flutter/navigation";

pub fn pop_route() -> Effect {
    Effect::Send { channel: CHANNEL, bytes: encode_call("popRoute", Value::Null) }
}
