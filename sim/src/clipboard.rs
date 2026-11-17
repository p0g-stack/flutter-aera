//! AERA's clipboard transfer (`features/plugin_api/clipboard.hpp`): one
//! acknowledged packet at a time; a write replaces the clipboard only once
//! every declared byte arrived.
//!
//! One deviation: [`Message::text`] is a `String`, so a read chunk ends on
//! the last character boundary within AERA's 1023 bytes instead of
//! possibly mid-character. Offsets and totals stay byte counts.

use flutter_aera::host::{kind, Message};

/// `kMaxClipboardBytes`.
pub const MAX_BYTES: u32 = 1024 * 1024;
/// `kClipboardContinue` in a `GET_CLIPBOARD`'s `flags`.
pub const CONTINUE: u32 = 1;
/// `ClipboardResultFlags`.
pub const READY: u32 = 1 << 0;
pub const MORE: u32 = 1 << 1;
pub const COMPLETE: u32 = 1 << 2;
pub const ERROR: u32 = 1 << 3;
/// `sizeof(Message::text) - 1`.
const CHUNK: usize = 1023;

#[derive(Default)]
pub struct Clipboard {
    /// The device clipboard.
    pub text: String,
    read_id: u32,
    write_id: u32,
    expected: u32,
    read_offset: u32,
    snapshot: String,
    pending: String,
}

impl Clipboard {
    pub fn reset(&mut self) {
        self.read_id = 0;
        self.write_id = 0;
        self.expected = 0;
        self.read_offset = 0;
        self.snapshot.clear();
        self.pending.clear();
    }

    fn error(&mut self, request_id: u32, reason: &str) -> Message {
        self.reset();
        Message { title: reason.into(), ..Message::with(kind::CLIPBOARD_RESULT, request_id, 0, ERROR) }
    }

    /// The `CLIPBOARD_RESULT` for one `GET_CLIPBOARD`, `SET_CLIPBOARD` or
    /// `CLIPBOARD_CHUNK`.
    pub fn handle(&mut self, request: &Message, allowed: bool) -> Message {
        let id = request.request_id;
        let mut reply = Message::with(kind::CLIPBOARD_RESULT, id, 0, 0);
        if !allowed {
            return self.error(id, "Clipboard is available only to the active plugin view.");
        }
        if id == 0 || !request.title.is_empty() {
            return self.error(id, "Invalid clipboard request.");
        }
        if request.kind == kind::GET_CLIPBOARD {
            if !request.text.is_empty() || (request.flags != 0 && request.flags != CONTINUE) {
                return self.error(id, "Invalid clipboard read.");
            }
            if request.flags == 0 {
                if request.value != 0 {
                    return self.error(id, "Clipboard reads must start at offset zero.");
                }
                self.reset();
                if self.text.len() > MAX_BYTES as usize {
                    return self.error(id, "Clipboard text exceeds the limit or is not valid UTF-8.");
                }
                self.snapshot = self.text.clone();
                self.read_id = id;
            } else if self.read_id != id || request.value != self.read_offset {
                return self.error(id, "Clipboard read offset or request ID changed.");
            }
            let start = request.value as usize;
            let mut end = (start + CHUNK).min(self.snapshot.len());
            while !self.snapshot.is_char_boundary(end) {
                end -= 1;
            }
            reply.text = self.snapshot[start..end].to_owned();
            reply.value = self.snapshot.len() as u32;
            self.read_offset = end as u32;
            reply.flags = if end < self.snapshot.len() { MORE } else { COMPLETE };
            if reply.flags == COMPLETE {
                self.reset();
            }
            return reply;
        }
        if request.kind == kind::SET_CLIPBOARD {
            if request.flags != 0 || !request.text.is_empty() || request.value > MAX_BYTES {
                return self.error(id, "Invalid clipboard size or write request.");
            }
            self.reset();
            self.expected = request.value;
            self.write_id = id;
            if self.expected != 0 {
                reply.flags = READY;
                return reply;
            }
            self.text.clear();
            self.reset();
            reply.flags = COMPLETE;
            return reply;
        }
        if request.kind != kind::CLIPBOARD_CHUNK
            || request.flags != 0
            || request.request_id != self.write_id
            || self.write_id == 0
            || request.value as usize != self.pending.len()
        {
            return self.error(id, "Clipboard write offset or request ID changed.");
        }
        let count = request.text.len();
        if count == 0 || count > self.expected as usize - self.pending.len() {
            return self.error(id, "Clipboard chunk exceeds the declared size.");
        }
        self.pending.push_str(&request.text);
        reply.value = self.pending.len() as u32;
        if self.pending.len() < self.expected as usize {
            reply.flags = READY;
            return reply;
        }
        self.text = std::mem::take(&mut self.pending);
        self.reset();
        reply.flags = COMPLETE;
        reply
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: u32, id: u32, value: u32, flags: u32, text: &str) -> Message {
        Message { text: text.into(), ..Message::with(kind, id, value, flags) }
    }

    #[test]
    fn writes_land_whole_and_reads_come_back_in_chunks() {
        let mut c = Clipboard::default();
        let big = "é".repeat(700); // 1400 bytes
        let r = c.handle(&request(kind::SET_CLIPBOARD, 5, big.len() as u32, 0, ""), true);
        assert_eq!(r.flags, READY);
        let first = &big[..1000];
        let r = c.handle(&request(kind::CLIPBOARD_CHUNK, 5, 0, 0, first), true);
        assert_eq!((r.flags, r.value), (READY, 1000));
        assert!(c.text.is_empty());
        let r = c.handle(&request(kind::CLIPBOARD_CHUNK, 5, 1000, 0, &big[1000..]), true);
        assert_eq!(r.flags, COMPLETE);
        assert_eq!(c.text, big);

        let r = c.handle(&request(kind::GET_CLIPBOARD, 6, 0, 0, ""), true);
        assert_eq!((r.flags, r.value, r.text.len()), (MORE, 1400, 1022));
        let r = c.handle(&request(kind::GET_CLIPBOARD, 6, 1022, CONTINUE, ""), true);
        assert_eq!((r.flags, r.text.len()), (COMPLETE, 378));
    }

    #[test]
    fn a_modal_or_a_bad_offset_is_an_error() {
        let mut c = Clipboard::default();
        assert_eq!(c.handle(&request(kind::GET_CLIPBOARD, 1, 0, 0, ""), false).flags, ERROR);
        assert_eq!(c.handle(&request(kind::GET_CLIPBOARD, 1, 4, 0, ""), true).flags, ERROR);
        assert_eq!(c.handle(&request(kind::CLIPBOARD_CHUNK, 1, 0, 0, "x"), true).flags, ERROR);
    }
}
