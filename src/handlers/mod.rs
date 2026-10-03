//! One handler per standard Flutter channel.
//!
//! Each handler is plain logic: it takes a message (from the framework or
//! from AERA) and returns the reply plus [`Effect`]s, so it is tested without
//! an engine or a host. `engine.rs` carries the effects out.
//!
//! Stub policy (AGENTS.md): what AERA can't do answers not-implemented (the
//! empty reply) or is a no-op documented here, never a custom channel. The
//! one exception is [`window`], padding, which the embedder API cannot carry.
//!
//! | Channel | Codec | Here | Linux GTK | flutter-pi |
//! | --- | --- | --- | --- | --- |
//! | `flutter/platform` | JSON method | [`platform`] | clipboard, pop, exit, haptics no-op | pop, orientation, overlays no-op |
//! | `flutter/textinput` | JSON method | [`textinput`] + `ime.rs` | GTK IM context | own model, hardware keys |
//! | `flutter/mousecursor` | standard method | [`mousecursor`]: no-op (touch only) | GDK cursors | DRM cursor |
//! | `flutter/navigation` | JSON method | [`navigation`]: AERA Back → `popRoute` | not used | not used |
//! | `flutter/lifecycle` | string | [`lifecycle`]: AERA `LIFECYCLE` | window focus/visibility | always resumed |
//! | `flutter/settings` | JSON message | [`settings`]: sent once at start; brightness from `AERA_APPEARANCE`, text scale 1.0, clock from AERA's saved `tw_military_time` | GSettings | sent once, light |
//! | `flutter/keyboard` | standard method | [`keyboard`]: `getKeyboardState` → no keys down (no hardware keyboard) | pressed keys from GDK | not handled |
//! | locales | embedder API | `LANG` (AERA sets it from its locale), else `en-US` | GLib languages | `LANG` |
//! | `flutter/accessibility` | standard message | not-implemented; semantics never enabled (AERA has no screen reader) | ATK | not handled |
//! | `flutter/keyevent` | JSON message | not sent: AERA's keyboard commits text, as an IME | GDK key events | evdev keys |
//! | `io.material.plugins/dynamic_color` | standard method | [`dynamic_color`]: `getAccentColor` = AERA's saved accent; in place of the stock plugin's GTK half | GTK plugin: portal or theme accent | not handled |
//! | `dev.flutter.pigeon.file_selector_linux…showFileChooser` | Pigeon (standard) | [`file_selector`]: AERA's picker (`kPickFiles`), answered when AERA is; in place of the stock plugin's GTK half | GTK plugin: `GtkFileChooserNative` | not handled |
//! | `aera/window` | JSON message | [`window`]: the view's padding (edge strips, rounded corners), for the app's AERA shell to apply; custom because the embedder API has no padding | none (zero padding) | none |
//! | `flutter/restoration` | standard method | not-implemented (as GTK) | not-implemented | not-implemented |
//! | anything else | | not-implemented | | |

pub mod codec;
pub mod dynamic_color;
pub mod file_selector;
pub mod keyboard;
pub mod lifecycle;
pub mod mousecursor;
pub mod navigation;
pub mod platform;
pub mod settings;
pub mod standard;
pub mod textinput;
pub mod window;

use crate::host::Message;
use crate::ime::Ime;

/// Something a handler needs done outside itself.
#[derive(Debug, PartialEq)]
pub enum Effect {
    /// Send this to AERA.
    Host(Message),
    /// Send a platform message to the framework.
    Send { channel: &'static str, bytes: Vec<u8> },
    /// The bottom inset changed (AERA's keyboard); resend window metrics.
    BottomInset(u32),
    /// Answer a platform message the framework sent earlier, through the
    /// engine's response handle for it.
    Respond { response: usize, bytes: Vec<u8> },
    /// Leave: tell AERA `CLOSE` and shut the engine down.
    Exit,
}

/// State shared by the handlers for one engine.
#[derive(Default)]
pub struct Handlers {
    pub ime: Ime,
    pub platform: platform::Platform,
    /// AERA's saved settings, read once at start.
    pub settings: crate::aera_settings::Settings,
    /// `HELLO_ACK` features.
    pub features: u32,
    pub file_selector: file_selector::FileSelector,
}

impl Handlers {
    /// A platform message from the framework, with the engine's response
    /// handle (`response`). Returns the reply bytes (empty =
    /// not-implemented), or `None` when an [`Effect::Respond`] answers later,
    /// and effects.
    pub fn on_message(&mut self, channel: &str, bytes: &[u8], response: usize) -> (Option<Vec<u8>>, Vec<Effect>) {
        let mut effects = vec![];
        if channel == file_selector::CHANNEL {
            let reply = self.file_selector.call(bytes, response, self.features, &mut effects);
            return (reply, effects);
        }
        let reply = match channel {
            platform::CHANNEL => self.platform.handle(bytes, &mut effects),
            textinput::CHANNEL => textinput::handle(&mut self.ime, bytes, &mut effects),
            mousecursor::CHANNEL => mousecursor::handle(bytes),
            keyboard::CHANNEL => keyboard::handle(bytes),
            dynamic_color::CHANNEL => dynamic_color::handle(bytes, self.settings.accent()),
            _ => codec::not_implemented(),
        };
        (Some(reply), effects)
    }

    /// A message from AERA that a handler owns. Input and frames are the
    /// view's; everything else lands here.
    pub fn on_host(&mut self, message: &Message) -> Vec<Effect> {
        use crate::host::kind;
        let mut effects = vec![];
        match message.kind {
            kind::BACK => effects.push(navigation::pop_route()),
            kind::KEY => textinput::on_key(&mut self.ime, message.value, &mut effects),
            kind::KEYBOARD_INSET => effects.push(Effect::BottomInset(message.value)),
            kind::LIFECYCLE => effects.extend(lifecycle::from_host(message.value)),
            kind::OPERATION_RESULT => self.file_selector.on_result(message, &mut effects),
            _ => {}
        }
        effects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_channels_are_not_implemented() {
        let mut h = Handlers::default();
        let (reply, effects) = h.on_message("flutter/restoration", b"\x07\x03get", 0);
        assert!(reply.unwrap().is_empty() && effects.is_empty());
    }
}
