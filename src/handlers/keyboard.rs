//! `flutter/keyboard` (StandardMethodCodec). As GTK: `getKeyboardState`
//! answers the keys held down, which is none, since AERA sends committed
//! characters rather than key events. The framework asks once at start to
//! sync `HardwareKeyboard`.

use super::codec;

pub const CHANNEL: &str = "flutter/keyboard";

/// The method name of a StandardMethodCodec call (a short UTF-8 string).
pub(super) fn method(bytes: &[u8]) -> Option<&str> {
    match bytes {
        [7, len, rest @ ..] if (*len as usize) < 254 && rest.len() >= *len as usize => {
            std::str::from_utf8(&rest[..*len as usize]).ok()
        }
        _ => None,
    }
}

pub fn handle(bytes: &[u8]) -> Vec<u8> {
    match method(bytes) {
        // Success envelope (0), an empty map (13, size 0).
        Some("getKeyboardState") => vec![0, 13, 0],
        _ => codec::not_implemented(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_keys_held() {
        let mut call = vec![7, 16];
        call.extend_from_slice(b"getKeyboardState");
        call.push(0); // null arguments
        assert_eq!(handle(&call), [0, 13, 0]);
        assert!(handle(&[7, 3, b'f', b'o', b'o', 0]).is_empty());
    }
}
