//! `flutter/mousecursor`: AERA is touch only, so `activateSystemCursor` is a
//! documented no-op that succeeds (the framework calls it on every hover
//! change and would log a missing plugin otherwise).

pub const CHANNEL: &str = "flutter/mousecursor";

/// The standard method codec's success envelope with a null result.
const SUCCESS_NULL: &[u8] = &[0, 0];

pub fn handle(_bytes: &[u8]) -> Vec<u8> {
    SUCCESS_NULL.to_vec()
}
