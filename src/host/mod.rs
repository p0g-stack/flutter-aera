//! AERA's generic pixel + GPU plugin host (Host API 3), as far as this
//! embedder relies on it.
//!
//! Host API 2 is AERA's (`vendor/aera/protocol.hpp`, verbatim). Host API 3
//! is our patch series for AERA (`third_party/aera/`), whose header is
//! vendored as `vendor/aera/protocol_v3.hpp` until AERA takes it. That
//! header, this module and `spec/host.md` change together; `tests/drift.rs`
//! keeps the numbers in step.

pub mod surface;
pub mod transport;

pub use surface::{Geometry, Slots};
pub use transport::Control;

/// `A2PI`, Host API 2.
pub const MAGIC: u32 = 0x4132_5049;
/// Host API 3 (`kProtocolVersion3`); Host API 2 is 2.
pub const PROTOCOL_VERSION: u32 = 3;
pub const TITLE_LEN: usize = 96;
pub const TEXT_LEN: usize = 1024;
/// `sizeof(aeraui::plugin_api::Message)`, Host API 2.
pub const MESSAGE_LEN: usize = 6 * 4 + TITLE_LEN + TEXT_LEN;

/// Host API 2: the control socket, also named in `AERA_PLUGIN_FD`.
pub const CONTROL_FD: i32 = 4;
pub const CONTROL_FD_ENV: &str = "AERA_PLUGIN_FD";
/// Host API 3: the frame memfd, also named in `AERA_SURFACE_FD`.
pub const SURFACE_FD: i32 = 3;
pub const SURFACE_FD_ENV: &str = "AERA_SURFACE_FD";
/// Host API 2: where the payload was extracted.
pub const ROOT_ENV: &str = "AERA_PLUGIN_ROOT";
/// Host API 3: persistent per-plugin data directory.
pub const DATA_ENV: &str = "AERA_PLUGIN_DATA";

/// `HELLO_ACK` feature bits.
pub mod feature {
    /// Host API 2.
    pub const METRICS: u32 = 1 << 0;
    /// Host API 2.
    pub const BACK_NAVIGATION: u32 = 1 << 1;
    /// Host API 3.
    pub const PIXEL_SURFACE: u32 = 1 << 2;
    /// Host API 3.
    pub const KEYBOARD_INSET: u32 = 1 << 3;
}

/// Message kinds. Host API 2 numbers are unchanged; Host API 3 kinds are
/// appended after each direction's last Host API 2 kind.
pub mod kind {
    // Plugin to host, Host API 2.
    pub const HELLO: u32 = 1;
    pub const SET_STATUS: u32 = 5;
    pub const CLOSE: u32 = 7;
    /// Host API 2's last plugin kind.
    pub const SET_BACK_ACTION: u32 = 11;
    // Plugin to host, Host API 3.
    /// A frame is ready: `request_id` sequence, `value` slot.
    pub const PRESENT: u32 = 12;
    /// Show AERA's keyboard: `value` input purpose ([`super::purpose`]),
    /// `flags` 1 for multiline.
    pub const KEYBOARD_SHOW: u32 = 13;
    pub const KEYBOARD_HIDE: u32 = 14;

    // Host to plugin, Host API 2.
    pub const HELLO_ACK: u32 = 64;
    pub const LIFECYCLE: u32 = 66;
    /// Host API 2's last host kind.
    pub const OPERATION_RESULT: u32 = 67;
    // Host to plugin, Host API 3.
    /// Surface geometry: `value` width, `flags` height, `request_id` stride,
    /// `title` pixel format, `text` `slots=N scale=S refresh=HZ`.
    pub const SURFACE: u32 = 68;
    /// The frame `request_id`'s slot is the plugin's again: a newer frame
    /// replaced it on screen, or superseded it before it was shown.
    pub const FRAME_DONE: u32 = 69;
    /// `request_id` pointer, `value` x, `flags` y, in surface pixels.
    pub const TOUCH_DOWN: u32 = 70;
    pub const TOUCH_MOVE: u32 = 71;
    pub const TOUCH_UP: u32 = 72;
    /// `value` Unicode code point; 8 backspace, 13 enter or submit.
    pub const KEY: u32 = 73;
    /// AERA's back gesture.
    pub const BACK: u32 = 74;
    /// AERA's keyboard covers `value` surface pixels at the bottom; 0 hidden.
    pub const KEYBOARD_INSET: u32 = 75;

    pub fn from_plugin(kind: u32) -> bool {
        (HELLO..=KEYBOARD_HIDE).contains(&kind)
    }

    pub fn from_host(kind: u32) -> bool {
        (HELLO_ACK..=KEYBOARD_INSET).contains(&kind)
    }
}

/// Host API 2 lifecycle values.
pub mod lifecycle {
    pub const RESUME: u32 = 1;
    pub const PAUSE: u32 = 2;
    pub const STOP: u32 = 3;
}

/// Host API 3: keyboard input purposes, as AERA Browser's keyboard has them.
pub mod purpose {
    pub const TEXT: u32 = 0;
    pub const DIGITS: u32 = 2;
}

/// One `aeraui::plugin_api::Message`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Message {
    pub kind: u32,
    pub request_id: u32,
    pub value: u32,
    pub flags: u32,
    pub title: String,
    pub text: String,
}

fn put_str(out: &mut [u8], text: &str) {
    // Leave room for the NUL; cut on a character boundary.
    let mut end = text.len().min(out.len() - 1);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    out[..end].copy_from_slice(&text.as_bytes()[..end]);
}

fn get_str(bytes: &[u8]) -> Option<String> {
    let end = bytes.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

impl Message {
    pub fn new(kind: u32) -> Message {
        Message { kind, ..Default::default() }
    }

    pub fn with(kind: u32, request_id: u32, value: u32, flags: u32) -> Message {
        Message { kind, request_id, value, flags, ..Default::default() }
    }

    pub fn encode(&self) -> [u8; MESSAGE_LEN] {
        let mut out = [0u8; MESSAGE_LEN];
        let words = [MAGIC, PROTOCOL_VERSION, self.kind, self.request_id, self.value, self.flags];
        for (i, word) in words.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        put_str(&mut out[24..24 + TITLE_LEN], &self.title);
        put_str(&mut out[24 + TITLE_LEN..], &self.text);
        out
    }

    /// Decodes one packet; `None` for anything Host API 2's `Valid()` would
    /// reject, except the direction check, which the caller makes.
    pub fn decode(bytes: &[u8]) -> Option<Message> {
        if bytes.len() != MESSAGE_LEN {
            return None;
        }
        let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        if word(0) != MAGIC || word(1) != PROTOCOL_VERSION {
            return None;
        }
        Some(Message {
            kind: word(2),
            request_id: word(3),
            value: word(4),
            flags: word(5),
            title: get_str(&bytes[24..24 + TITLE_LEN])?,
            text: get_str(&bytes[24 + TITLE_LEN..])?,
        })
    }

    /// Host API 2 reads the supported range from `value` (min) and `flags`
    /// (max).
    pub fn hello() -> Message {
        Message::with(kind::HELLO, 0, PROTOCOL_VERSION, PROTOCOL_VERSION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let m = Message { kind: kind::SURFACE, request_id: 4320, value: 1080, flags: 2100, title: "BGRA8888".into(), text: "slots=3 scale=3".into() };
        assert_eq!(Message::decode(&m.encode()), Some(m));
    }

    #[test]
    fn rejects_wrong_magic_version_and_size() {
        let mut bytes = Message::hello().encode();
        assert!(Message::decode(&bytes[..MESSAGE_LEN - 1]).is_none());
        bytes[4] = 2;
        assert!(Message::decode(&bytes).is_none());
        bytes[4] = 3;
        bytes[0] = 0;
        assert!(Message::decode(&bytes).is_none());
    }

    #[test]
    fn rejects_unterminated_strings() {
        let mut bytes = Message::hello().encode();
        bytes[24..24 + TITLE_LEN].fill(b'a');
        assert!(Message::decode(&bytes).is_none());
    }

    #[test]
    fn truncates_on_char_boundary() {
        let long = "é".repeat(TITLE_LEN);
        let m = Message { title: long, ..Message::new(kind::SET_STATUS) };
        let back = Message::decode(&m.encode()).unwrap();
        assert!(back.title.len() < TITLE_LEN && back.title.chars().all(|c| c == 'é'));
    }

    #[test]
    fn directions() {
        assert!(kind::from_plugin(kind::PRESENT) && !kind::from_plugin(kind::FRAME_DONE));
        assert!(kind::from_host(kind::KEYBOARD_INSET) && !kind::from_host(kind::CLOSE));
    }
}
