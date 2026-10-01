//! `io.material.plugins/dynamic_color` (StandardMethodCodec), the channel of
//! the stock `dynamic_color` plugin. On Linux its native half is a GTK
//! plugin, which flutter-aera does not load, so the embedder answers in its
//! place, as that plugin does: `getAccentColor` is the accent as opaque
//! ARGB (here AERA's saved `aera_theme_accent`); everything else, including
//! `getCorePalette`, is not-implemented, so the app derives a scheme from the
//! accent exactly as on a Linux desktop.

use super::{codec, keyboard};

pub const CHANNEL: &str = "io.material.plugins/dynamic_color";

pub fn handle(bytes: &[u8], accent: u32) -> Vec<u8> {
    match keyboard::method(bytes) {
        Some("getAccentColor") => {
            // Success envelope (0), int64 (4), as GTK's fl_value_new_int.
            let argb = 0xff00_0000u64 | u64::from(accent & 0xff_ffff);
            let mut reply = vec![0, 4];
            reply.extend_from_slice(&(argb as i64).to_le_bytes());
            reply
        }
        _ => codec::not_implemented(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_as_opaque_argb() {
        let mut call = vec![7, 14];
        call.extend_from_slice(b"getAccentColor");
        call.push(0);
        let reply = handle(&call, 0x16c8ff);
        assert_eq!(reply[..2], [0, 4]);
        assert_eq!(i64::from_le_bytes(reply[2..].try_into().unwrap()), 0xff16c8ff);
        let mut palette = vec![7, 14];
        palette.extend_from_slice(b"getCorePalette");
        palette.push(0);
        assert!(handle(&palette, 0x16c8ff).is_empty());
    }
}
