//! `aera/window` (JSONMessageCodec): the view's padding, the one custom
//! channel here, because no standard one can carry it. Flutter's embedder
//! API sets only `physical_view_inset_*` (`viewInsets`, the keyboard), never
//! `physical_padding_*` (`viewPadding`, what `SafeArea` and
//! `MediaQuery.padding` read), so an embedder cannot report padding the way
//! Android's does. The app's AERA shell applies this message as its root
//! `MediaQuery` padding; an app without that shell sees zero padding, as on
//! Linux desktop.
//!
//! The embedder sends `{"viewPadding":{"left":L,"top":T,"right":R,"bottom":B}}`
//! (physical pixels, like `FlutterView.viewPadding`) with every window
//! metrics update, and answers any message on the channel with the current
//! one. The engine keeps the last message for a channel no one listens on
//! yet, so a shell that starts listening late still gets it.

use super::Effect;

pub const CHANNEL: &str = "aera/window";

/// Padding in physical pixels: left, top, right, bottom.
pub type Padding = [f64; 4];

pub fn message(padding: Padding) -> Vec<u8> {
    let [left, top, right, bottom] = padding;
    format!(r#"{{"viewPadding":{{"left":{left},"top":{top},"right":{right},"bottom":{bottom}}}}}"#).into_bytes()
}

pub fn update(padding: Padding) -> Effect {
    Effect::Send { channel: CHANNEL, bytes: message(padding) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_as_json() {
        assert_eq!(
            String::from_utf8(message([32.0, 0.0, 32.0, 60.0])).unwrap(),
            r#"{"viewPadding":{"left":32,"top":0,"right":32,"bottom":60}}"#
        );
    }
}
