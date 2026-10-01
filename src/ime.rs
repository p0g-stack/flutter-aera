//! The text-input model behind `flutter/textinput`.
//!
//! AERA's keyboard is a soft keyboard that sends committed characters, as an
//! Android IME commits text, so keys go straight into the editing state
//! (Android's `InputConnection` path) rather than through key events. Offsets
//! are UTF-16 code units, as Flutter's are.

use serde_json::{json, Value};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ime {
    /// The framework's client id while a field is attached.
    pub client: Option<i64>,
    input_type: String,
    input_action: String,
    text: String,
    base: usize,
    extent: usize,
}

/// What a key did.
#[derive(Debug, PartialEq)]
pub enum KeyResult {
    /// The editing state changed; send `TextInputClient.updateEditingState`.
    Edited,
    /// Enter on a single-line field: `TextInputClient.performAction` with
    /// this action.
    Action(String),
    /// No field is attached.
    Ignored,
}

fn utf16_to_byte(text: &str, offset: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units >= offset {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}

fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

impl Ime {
    pub fn set_client(&mut self, client: i64, config: &Value) {
        self.client = Some(client);
        self.input_type = config["inputType"]["name"].as_str().unwrap_or("TextInputType.text").to_owned();
        self.input_action = config["inputAction"].as_str().unwrap_or("TextInputAction.done").to_owned();
    }

    pub fn clear_client(&mut self) {
        self.client = None;
    }

    pub fn multiline(&self) -> bool {
        self.input_type == "TextInputType.multiline"
    }

    /// Whether AERA's digits keyboard fits this field.
    pub fn digits(&self) -> bool {
        matches!(self.input_type.as_str(), "TextInputType.number" | "TextInputType.phone" | "TextInputType.datetime")
    }

    pub fn set_state(&mut self, state: &Value) {
        self.text = state["text"].as_str().unwrap_or("").to_owned();
        let len = utf16_len(&self.text) as i64;
        let clamp = |v: &Value| v.as_i64().unwrap_or(-1).clamp(-1, len);
        let (base, extent) = (clamp(&state["selectionBase"]), clamp(&state["selectionExtent"]));
        // -1 means no selection; put the caret at the end, as Android does.
        if base < 0 || extent < 0 {
            self.base = len as usize;
            self.extent = len as usize;
        } else {
            self.base = base as usize;
            self.extent = extent as usize;
        }
    }

    pub fn state(&self) -> Value {
        json!({
            "text": self.text,
            "selectionBase": self.base,
            "selectionExtent": self.extent,
            "selectionAffinity": "TextAffinity.downstream",
            "selectionIsDirectional": false,
            "composingBase": -1,
            "composingExtent": -1,
        })
    }

    fn selection(&self) -> (usize, usize) {
        (self.base.min(self.extent), self.base.max(self.extent))
    }

    fn replace_selection(&mut self, with: &str) {
        let (start, end) = self.selection();
        let (a, b) = (utf16_to_byte(&self.text, start), utf16_to_byte(&self.text, end));
        self.text.replace_range(a..b, with);
        self.base = start + utf16_len(with);
        self.extent = self.base;
    }

    /// One key from AERA's keyboard: a code point, 8 backspace, 13 enter.
    pub fn key(&mut self, code: u32) -> KeyResult {
        if self.client.is_none() {
            return KeyResult::Ignored;
        }
        match code {
            8 => {
                let (start, end) = self.selection();
                if start == end {
                    if start == 0 {
                        return KeyResult::Edited;
                    }
                    // Delete the whole character before the caret.
                    let byte = utf16_to_byte(&self.text, start);
                    let previous = self.text[..byte].chars().next_back().unwrap();
                    self.base = start - previous.len_utf16();
                    self.extent = start;
                }
                self.replace_selection("");
                KeyResult::Edited
            }
            13 | 10 if !self.multiline() => KeyResult::Action(self.input_action.clone()),
            13 | 10 => {
                self.replace_selection("\n");
                KeyResult::Edited
            }
            code => match char::from_u32(code) {
                Some(c) if !c.is_control() => {
                    self.replace_selection(c.encode_utf8(&mut [0; 4]));
                    KeyResult::Edited
                }
                _ => KeyResult::Edited,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attached(kind: &str) -> Ime {
        let mut ime = Ime::default();
        ime.set_client(7, &json!({ "inputType": { "name": kind }, "inputAction": "TextInputAction.search" }));
        ime
    }

    #[test]
    fn typing_and_backspace() {
        let mut ime = attached("TextInputType.text");
        for c in "hé😀".chars() {
            assert_eq!(ime.key(c as u32), KeyResult::Edited);
        }
        assert_eq!(ime.state()["text"], "hé😀");
        assert_eq!(ime.state()["selectionBase"], 4);
        ime.key(8);
        assert_eq!(ime.state()["text"], "hé");
        assert_eq!(ime.state()["selectionBase"], 2);
    }

    #[test]
    fn enter_submits_single_line_and_breaks_multiline() {
        let mut ime = attached("TextInputType.text");
        assert_eq!(ime.key(13), KeyResult::Action("TextInputAction.search".into()));
        let mut ime = attached("TextInputType.multiline");
        assert_eq!(ime.key(13), KeyResult::Edited);
        assert_eq!(ime.state()["text"], "\n");
    }

    #[test]
    fn replaces_the_selection() {
        let mut ime = attached("TextInputType.text");
        ime.set_state(&json!({ "text": "hello", "selectionBase": 1, "selectionExtent": 4 }));
        ime.key('a' as u32);
        assert_eq!(ime.state()["text"], "hao");
        assert_eq!(ime.state()["selectionBase"], 2);
    }

    #[test]
    fn ignored_without_a_client() {
        assert_eq!(Ime::default().key('a' as u32), KeyResult::Ignored);
    }

    #[test]
    fn digits_keyboard() {
        assert!(attached("TextInputType.number").digits());
        assert!(!attached("TextInputType.text").digits());
    }
}
