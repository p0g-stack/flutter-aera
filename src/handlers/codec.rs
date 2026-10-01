//! Flutter's JSON method and message codecs, the subset the standard
//! channels we answer use (`JSONMethodCodec`, `JSONMessageCodec`,
//! `StringCodec`).

use serde_json::{json, Value};

/// A decoded `JSONMethodCodec` call.
#[derive(Debug, PartialEq)]
pub struct MethodCall {
    pub method: String,
    pub args: Value,
}

pub fn decode_call(bytes: &[u8]) -> Option<MethodCall> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    Some(MethodCall { method: v.get("method")?.as_str()?.to_owned(), args: v.get("args").cloned().unwrap_or(Value::Null) })
}

pub fn encode_call(method: &str, args: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({ "method": method, "args": args })).unwrap()
}

pub fn success(result: Value) -> Vec<u8> {
    serde_json::to_vec(&json!([result])).unwrap()
}

pub fn error(code: &str, message: &str) -> Vec<u8> {
    serde_json::to_vec(&json!([code, message, null])).unwrap()
}

/// Flutter's not-implemented reply on every codec: an empty response.
pub fn not_implemented() -> Vec<u8> {
    Vec::new()
}

pub fn message(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls() {
        let bytes = encode_call("TextInput.show", Value::Null);
        assert_eq!(decode_call(&bytes), Some(MethodCall { method: "TextInput.show".into(), args: Value::Null }));
        assert!(decode_call(b"[1]").is_none());
        assert_eq!(success(json!(1)), b"[1]");
        assert_eq!(error("e", "m"), br#"["e","m",null]"#);
    }
}
