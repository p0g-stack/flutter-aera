//! AERA's Host API 2 (`vendor/aera/protocol.hpp`, verbatim upstream) as far
//! as this embedder relies on it, plus the feature-gated recovery
//! extensions we carry for AERA (`third_party/aera/patches-api2`).
//!
//! The control socket is the upstream `SOCK_SEQPACKET` on fd 4, one
//! 1144-byte [`Message`] per packet. Pixels and input travel on the optional
//! desktop surface: `OPEN_DESKTOP` → `DESKTOP_READY` with two `SCM_RIGHTS`
//! descriptors (a sealed double BGRA framebuffer and a `PlayMessage`
//! socket, [`play`]). That header, this module and `spec/host.md` change
//! together; `tests/drift.rs` keeps the numbers in step.

pub mod desktop;
pub mod input;
pub mod play;
pub mod surface;
pub mod transport;

pub use desktop::Desktop;
pub use play::{PlayChannel, PlayMessage};
pub use surface::{Geometry, Slots};
pub use transport::Control;

/// `A2PI`.
pub const MAGIC: u32 = 0x4132_5049;
/// `kProtocolVersion`.
pub const PROTOCOL_VERSION: u32 = 2;
pub const TITLE_LEN: usize = 96;
pub const TEXT_LEN: usize = 1024;
/// `sizeof(aeraui::plugin_api::Message)`.
pub const MESSAGE_LEN: usize = 6 * 4 + TITLE_LEN + TEXT_LEN;

/// The control socket. Upstream closes fd 3 and passes nothing else.
pub const CONTROL_FD: i32 = 4;
/// Upstream sets it to `4`; anything else is not Host API 2.
pub const CONTROL_FD_ENV: &str = "AERA_PLUGIN_FD";
/// Upstream sets it to `2`.
pub const HOST_API_ENV: &str = "AERA_HOST_API";
pub const HOST_API: &str = "2";
/// The argument upstream passes to the worker.
pub const HOST_API_ARG: &str = "--aera-host-api=2";
/// Where the payload was extracted.
pub const ROOT_ENV: &str = "AERA_PLUGIN_ROOT";
/// A persistent per-plugin data directory. Upstream does not set it; our
/// simulator and developer setups may.
pub const DATA_ENV: &str = "AERA_PLUGIN_DATA";

/// `kFeatureRequestMagic`: `HELLO.request_id` is this or'ed with the
/// optional features the worker asks for.
pub const FEATURE_REQUEST_MAGIC: u32 = 0xA2E2_0000;

/// `HELLO_ACK` feature bits.
pub mod feature {
    pub const METRICS: u32 = 1 << 0;
    pub const BACK_NAVIGATION: u32 = 1 << 1;
    pub const BACKGROUND: u32 = 1 << 2;
    pub const ACTIVITIES: u32 = 1 << 3;
    pub const RICH_PRESENTATION: u32 = 1 << 4;
    pub const PACKAGE_REVIEW: u32 = 1 << 5;
    pub const READABLE_PRESENTATION: u32 = 1 << 6;
    /// `OPEN_DESKTOP` / `DESKTOP_READY`.
    pub const DESKTOP: u32 = 1 << 7;
    /// `OPEN_DESKTOP` may name one of [`super::DESKTOP_SIZES`].
    pub const DESKTOP_RESOLUTION: u32 = 1 << 8;
    pub const CLIPBOARD: u32 = 1 << 9;
    /// Setup-page native text inputs.
    pub const TEXT_INPUT: u32 = 1 << 10;
    /// `PICK_FILE` / `FILE_PICKER_RESULT`.
    pub const FILE_PICKER: u32 = 1 << 11;
    /// Recovery extension: `PICK_FILE` over an open desktop,
    /// `SET_DESKTOP_KEYBOARD`, `DESKTOP_KEYBOARD_INSET` and Unicode keysyms
    /// from the desktop keyboard.
    pub const DESKTOP_INTEGRATION: u32 = 1 << 12;
    /// Recovery extension: `EDIT_TEXT_INPUT` / `DISMISS_TEXT_INPUT`, native
    /// modal editors over the desktop.
    pub const DESKTOP_TEXT_INPUT: u32 = 1 << 13;
}

/// Upstream `kOptionalFeatures` plus the recovery extensions: what a
/// `HELLO` feature request may carry.
pub const OPTIONAL_FEATURES: u32 = feature::BACKGROUND
    | feature::ACTIVITIES
    | feature::RICH_PRESENTATION
    | feature::PACKAGE_REVIEW
    | feature::READABLE_PRESENTATION
    | feature::DESKTOP
    | feature::DESKTOP_RESOLUTION
    | feature::CLIPBOARD
    | feature::TEXT_INPUT
    | feature::FILE_PICKER
    | feature::DESKTOP_INTEGRATION
    | feature::DESKTOP_TEXT_INPUT;

/// What this embedder asks for. The Flutter typing path is the desktop
/// keyboard plus `PLAY_KEY`, so `DESKTOP_TEXT_INPUT` (native modal editors)
/// is not requested.
pub const REQUESTED_FEATURES: u32 =
    feature::DESKTOP | feature::DESKTOP_RESOLUTION | feature::FILE_PICKER | feature::DESKTOP_INTEGRATION;

/// The desktop sizes `kFeatureDesktopResolution` permits, all landscape.
pub const DESKTOP_SIZES: [(u32, u32); 4] = [(640, 360), (960, 540), (1280, 720), (1920, 1080)];
/// What `OPEN_DESKTOP` with zero dimensions gets.
pub const DEFAULT_DESKTOP: (u32, u32) = (960, 540);

/// Message kinds, upstream numbering; recovery extensions are appended after
/// each direction's last upstream kind.
pub mod kind {
    // Worker to host.
    pub const HELLO: u32 = 1;
    pub const BEGIN_PAGE: u32 = 2;
    pub const ADD_BUTTON: u32 = 3;
    pub const COMMIT_PAGE: u32 = 4;
    pub const SET_STATUS: u32 = 5;
    pub const REQUEST_OPERATION: u32 = 6;
    pub const CLOSE: u32 = 7;
    pub const ADD_SECTION: u32 = 8;
    pub const ADD_METRIC: u32 = 9;
    pub const UPDATE_METRIC: u32 = 10;
    pub const SET_BACK_ACTION: u32 = 11;
    pub const SET_BACKGROUND: u32 = 12;
    pub const SET_ACTIVITY: u32 = 13;
    pub const SET_ACTIVITY_STORAGE: u32 = 14;
    pub const ADD_PROGRESS: u32 = 15;
    pub const UPDATE_PROGRESS: u32 = 16;
    pub const ADD_TILE: u32 = 17;
    pub const SET_PAGE_LAYOUT: u32 = 18;
    pub const SET_ACTIVITY_FILE: u32 = 19;
    pub const REVIEW_PACKAGE: u32 = 20;
    /// `request_id` nonzero, `value`/`flags` width/height or both zero.
    pub const OPEN_DESKTOP: u32 = 21;
    pub const GET_CLIPBOARD: u32 = 22;
    pub const SET_CLIPBOARD: u32 = 23;
    pub const CLIPBOARD_CHUNK: u32 = 24;
    pub const ADD_TEXT_INPUT: u32 = 25;
    pub const UPDATE_TEXT_INPUT: u32 = 26;
    pub const TEXT_INPUT_PLACEHOLDER: u32 = 27;
    /// Upstream's last worker kind.
    pub const PICK_FILE: u32 = 28;
    /// Extension ([`super::feature::DESKTOP_INTEGRATION`]): `request_id`
    /// nonzero, `value` 0 hide / 1 show, `flags`
    /// [`super::text_input::NUMBER`] or [`super::text_input::MULTILINE`].
    pub const SET_DESKTOP_KEYBOARD: u32 = 29;
    /// Extension ([`super::feature::DESKTOP_TEXT_INPUT`]): `request_id` a
    /// setup-page field id.
    pub const EDIT_TEXT_INPUT: u32 = 30;
    pub const DISMISS_TEXT_INPUT: u32 = 31;

    // Host to worker.
    pub const HELLO_ACK: u32 = 64;
    /// `request_id` the button id.
    pub const ACTION: u32 = 65;
    pub const LIFECYCLE: u32 = 66;
    pub const OPERATION_RESULT: u32 = 67;
    pub const BACKGROUND_RESULT: u32 = 68;
    pub const ACTIVITY_CANCEL: u32 = 69;
    pub const ACTIVITY_RESULT: u32 = 70;
    pub const ACTIVITY_FILE_RESULT: u32 = 71;
    /// Echoes `request_id`; `value`/`flags` width/height, two `SCM_RIGHTS`
    /// descriptors (memfd, then seqpacket). Zero dimensions and no
    /// descriptors on failure.
    pub const DESKTOP_READY: u32 = 72;
    pub const CLIPBOARD_RESULT: u32 = 73;
    pub const TEXT_INPUT_RESULT: u32 = 74;
    /// Upstream's last host kind.
    pub const FILE_PICKER_RESULT: u32 = 75;
    /// Extension ([`super::feature::DESKTOP_INTEGRATION`]): `value` the
    /// keyboard's bottom occlusion in desktop pixels.
    pub const DESKTOP_KEYBOARD_INSET: u32 = 76;

    pub fn from_plugin(kind: u32) -> bool {
        (HELLO..=DISMISS_TEXT_INPUT).contains(&kind)
    }

    pub fn from_host(kind: u32) -> bool {
        (HELLO_ACK..=DESKTOP_KEYBOARD_INSET).contains(&kind)
    }

    /// Whether `kind` may travel given the negotiated `features`: upstream
    /// kinds always, extension kinds only with their bit.
    pub fn negotiated(kind: u32, features: u32) -> bool {
        use super::feature::{DESKTOP_INTEGRATION, DESKTOP_TEXT_INPUT};
        match kind {
            SET_DESKTOP_KEYBOARD | DESKTOP_KEYBOARD_INSET => features & DESKTOP_INTEGRATION != 0,
            EDIT_TEXT_INPUT | DISMISS_TEXT_INPUT => features & DESKTOP_TEXT_INPUT != 0,
            _ => true,
        }
    }
}

/// `Lifecycle`.
pub mod lifecycle {
    pub const RESUME: u32 = 1;
    pub const PAUSE: u32 = 2;
    pub const STOP: u32 = 3;
}

/// Page `Flags` for buttons.
pub mod page {
    pub const PRIMARY: u32 = 1 << 0;
    pub const DESTRUCTIVE: u32 = 1 << 1;
    pub const DISABLED: u32 = 1 << 2;
}

/// `TextInputFlags` and `TextInputResultFlags`.
pub mod text_input {
    pub const PASSWORD: u32 = 1 << 0;
    pub const NUMBER: u32 = 1 << 1;
    pub const MULTILINE: u32 = 1 << 2;
    pub const DISABLED: u32 = 1 << 3;
    pub const SUBMITTED: u32 = 1 << 0;
    pub const CANCELLED: u32 = 1 << 1;
    pub const ERROR: u32 = 1 << 2;
    /// `kMaxInputCharacters`.
    pub const MAX_CHARACTERS: u32 = 255;
}

/// `FilePickerResultFlags`.
pub mod file_picker {
    pub const SELECTED: u32 = 1 << 0;
    pub const CANCELLED: u32 = 1 << 1;
    pub const ERROR: u32 = 1 << 2;
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

    /// Decodes one packet; `None` for anything upstream `Valid()` rejects,
    /// except the direction check, which the caller makes.
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

    /// `HELLO`: protocol range [2, 2] in `value`/`flags`, the feature request
    /// marker plus `features` in `request_id`.
    pub fn hello(features: u32) -> Message {
        Message::with(kind::HELLO, FEATURE_REQUEST_MAGIC | (features & OPTIONAL_FEATURES), PROTOCOL_VERSION, PROTOCOL_VERSION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let m = Message { kind: kind::SET_STATUS, request_id: 4320, value: 1080, flags: 2100, title: "t".into(), text: "body".into() };
        assert_eq!(Message::decode(&m.encode()), Some(m));
    }

    #[test]
    fn rejects_wrong_magic_version_and_size() {
        let mut bytes = Message::hello(0).encode();
        assert!(Message::decode(&bytes).is_some());
        assert!(Message::decode(&bytes[..MESSAGE_LEN - 1]).is_none());
        bytes[4] = 3;
        assert!(Message::decode(&bytes).is_none());
        bytes[4] = 2;
        bytes[0] = 0;
        assert!(Message::decode(&bytes).is_none());
    }

    #[test]
    fn rejects_unterminated_strings() {
        let mut bytes = Message::hello(0).encode();
        bytes[24..24 + TITLE_LEN].fill(b'a');
        assert!(Message::decode(&bytes).is_none());
        let mut bytes = Message::hello(0).encode();
        bytes[24 + TITLE_LEN..].fill(b'a');
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
    fn hello_requests_features_behind_the_marker() {
        let h = Message::hello(REQUESTED_FEATURES | 1 << 20);
        assert_eq!((h.kind, h.value, h.flags), (kind::HELLO, 2, 2));
        assert_eq!(h.request_id & 0xFFFF_0000, FEATURE_REQUEST_MAGIC);
        assert_eq!(h.request_id & 0xFFFF, REQUESTED_FEATURES);
        assert_eq!(REQUESTED_FEATURES & feature::DESKTOP_TEXT_INPUT, 0);
    }

    #[test]
    fn directions() {
        assert!(kind::from_plugin(kind::OPEN_DESKTOP) && kind::from_plugin(kind::DISMISS_TEXT_INPUT));
        assert!(!kind::from_plugin(kind::DESKTOP_READY) && !kind::from_plugin(32));
        assert!(kind::from_host(kind::DESKTOP_READY) && kind::from_host(kind::DESKTOP_KEYBOARD_INSET));
        assert!(!kind::from_host(kind::CLOSE) && !kind::from_host(77));
    }

    #[test]
    fn extensions_need_their_bit() {
        assert!(kind::negotiated(kind::DESKTOP_READY, 0));
        assert!(!kind::negotiated(kind::DESKTOP_KEYBOARD_INSET, feature::DESKTOP));
        assert!(kind::negotiated(kind::DESKTOP_KEYBOARD_INSET, feature::DESKTOP_INTEGRATION));
        assert!(!kind::negotiated(kind::EDIT_TEXT_INPUT, feature::DESKTOP_INTEGRATION));
        assert!(kind::negotiated(kind::DISMISS_TEXT_INPUT, feature::DESKTOP_TEXT_INPUT));
    }
}
