//! `file_selector`'s Linux channel
//! (`dev.flutter.pigeon.file_selector_linux.FileSelectorApi.showFileChooser`,
//! Pigeon over StandardMessageCodec). On a desktop its native half shows a
//! GTK file chooser, which flutter-aera does not load; the embedder answers
//! in its place with upstream Host API 2 `PickFile`/`FilePickerResult`.
//!
//! The desktop integration extension permits the picker over the desktop.
//! Without it this channel returns `unsupported`, as do multi-file and save
//! requests: upstream Files only selects one existing file or directory.
//! Extension filters use the upstream dotted, comma-separated syntax. MIME
//! only groups cannot be represented and remain unfiltered, as before.

use super::standard::{self, Value};
use super::Effect;
use crate::host::{feature, kind, Message, TEXT_LEN, TITLE_LEN};

pub const CHANNEL: &str = "dev.flutter.pigeon.file_selector_linux.FileSelectorApi.showFileChooser";

/// Pigeon's `PlatformFileChooserActionType`.
const OPEN: i64 = 0;
const CHOOSE_DIRECTORY: i64 = 1;
const SAVE: i64 = 2;

#[derive(Default)]
pub struct FileSelector {
    next_id: u32,
    pending: Option<Pending>,
}

struct Pending {
    id: u32,
    /// The engine's response handle for the call.
    response: usize,
}

/// AERA's picker request for one call, or why there is none.
pub fn request(bytes: &[u8]) -> Result<Message, &'static str> {
    let args = standard::decode(bytes).ok_or("malformed call")?;
    let args = args.list().ok_or("malformed call")?;
    let (Some(action), Some(options)) = (args.first(), args.get(1)) else { return Err("malformed call") };
    let Value::Int(action) = action.inner() else { return Err("malformed call") };
    let options = options.inner().list().ok_or("malformed call")?;
    let field = |i: usize| options.get(i).unwrap_or(&Value::Null);
    let multiple = field(4).bool() == Some(true);
    if multiple {
        return Err("unsupported: multiple selection is not provided by AERA Host API 2");
    }
    let mode = match *action {
        OPEN => 0,
        CHOOSE_DIRECTORY => 1,
        SAVE => return Err("unsupported: save selection is not provided by AERA Host API 2"),
        _ => return Err("unknown action"),
    };
    let mut extensions = vec![];
    let mut any = false;
    for group in field(0).list().unwrap_or_default() {
        let group = group.inner().list().unwrap_or_default();
        let listed: Vec<&str> = group.get(1).and_then(Value::list).unwrap_or_default().iter().filter_map(Value::str).collect();
        if listed.is_empty() || listed.contains(&"*") {
            any = true;
        }
        for pattern in listed {
            if pattern == "*" {
                continue;
            }
            let ext = pattern.trim_start_matches('*').trim_start_matches('.').to_ascii_lowercase();
            if ext.is_empty() || ext.len() > 31 || !ext.bytes().all(|c| c.is_ascii_alphanumeric() || b"_-.".contains(&c)) {
                return Err("invalid file extension filter");
            }
            let ext = format!(".{ext}");
            if !extensions.contains(&ext) {
                extensions.push(ext);
            }
        }
    }
    let mut m = Message::with(kind::PICK_FILE, 0, mode, 0);
    m.text = field(1).str().unwrap_or_default().to_owned();
    if !m.text.is_empty() && (!m.text.starts_with('/') || m.text.bytes().any(|b| b < 32 || b == 127)) {
        return Err("the starting folder must be an absolute path without control characters");
    }
    if mode == 0 && !any {
        if extensions.len() > 16 {
            return Err("too many file extension filters");
        }
        m.title = extensions.join(",");
    }
    if m.title.len() >= TITLE_LEN || m.text.len() >= TEXT_LEN {
        return Err("file selection request exceeds the host packet limit");
    }
    Ok(m)
}

impl FileSelector {
    /// A call from the framework. Returns the reply now, or `None` when it
    /// comes later through [`Effect::Respond`].
    pub fn call(&mut self, bytes: &[u8], response: usize, features: u32, effects: &mut Vec<Effect>) -> Option<Vec<u8>> {
        let required = feature::FILE_PICKER | feature::DESKTOP_INTEGRATION;
        if features & required != required {
            return Some(standard::pigeon_error("unsupported", "This AERA cannot open Files over the desktop."));
        }
        if self.pending.is_some() {
            return Some(standard::pigeon_error("busy", "A file picker is already open."));
        }
        let mut m = match request(bytes) {
            Ok(m) => m,
            Err(e) => return Some(match e.strip_prefix("unsupported: ") {
                Some(reason) => standard::pigeon_error("unsupported", reason),
                None => standard::pigeon_error("bad-args", e),
            }),
        };
        self.next_id = self.next_id.wrapping_add(1).max(1);
        m.request_id = self.next_id;
        self.pending = Some(Pending { id: m.request_id, response });
        effects.push(Effect::Host(m));
        None
    }

    /// Exactly one `FilePickerResult` completes a pending call.
    pub fn on_result(&mut self, m: &Message, effects: &mut Vec<Effect>) {
        if m.kind != kind::FILE_PICKER_RESULT {
            return;
        }
        let Some(p) = self.pending.as_ref().filter(|p| p.id == m.request_id) else { return };
        let bytes = match (m.value, m.flags) {
            (0, 1) if m.text.starts_with('/') && !m.text.bytes().any(|b| b < 32 || b == 127) => {
                standard::pigeon_success(Value::List(vec![Value::Str(m.text.clone())]))
            }
            (0, 2) if m.text.is_empty() => standard::pigeon_success(Value::List(vec![])),
            (0, 4) => standard::pigeon_error("aera", &m.text),
            _ => standard::pigeon_error("aera", "Invalid file selection result."),
        };
        let response = p.response;
        self.pending = None;
        effects.push(Effect::Respond { response, bytes });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(action: i64, groups: Vec<Vec<&str>>, folder: Option<&str>, name: Option<&str>, multiple: Option<bool>) -> Vec<u8> {
        let groups = Value::List(
            groups
                .into_iter()
                .map(|exts| {
                    Value::Custom(130, Box::new(Value::List(vec![
                        Value::Str(String::new()),
                        Value::List(exts.into_iter().map(|e| Value::Str(e.into())).collect()),
                        Value::List(vec![]),
                    ])))
                })
                .collect(),
        );
        let opt = |s: Option<&str>| s.map_or(Value::Null, |s| Value::Str(s.into()));
        standard::encode(&Value::List(vec![
            Value::Custom(129, Box::new(Value::Int(action))),
            Value::Custom(131, Box::new(Value::List(vec![groups, opt(folder), opt(name), Value::Null, multiple.map_or(Value::Null, Value::Bool), Value::Null]))),
        ]))
    }

    #[test]
    fn maps_calls_to_upstream_pick_requests() {
        let m = request(&call(OPEN, vec![vec!["*.ZIP", "*.img"], vec!["*.zip"]], Some("/sdcard/Download"), None, Some(false))).unwrap();
        assert_eq!((m.kind, m.value, m.flags), (kind::PICK_FILE, 0, 0));
        assert_eq!((m.title.as_str(), m.text.as_str()), (".zip,.img", "/sdcard/Download"));
        assert_eq!(request(&call(CHOOSE_DIRECTORY, vec![], None, None, None)).unwrap().value, 1);
        assert!(request(&call(OPEN, vec![vec!["*"]], None, None, None)).unwrap().title.is_empty());
        assert!(request(&call(OPEN, vec![], None, None, Some(true))).unwrap_err().starts_with("unsupported:"));
        assert!(request(&call(SAVE, vec![], None, Some("notes.txt"), None)).unwrap_err().starts_with("unsupported:"));
        assert!(request(&call(OPEN, vec![], Some("relative"), None, None)).is_err());
        assert!(request(&call(OPEN, vec![vec!["*.bad/ext"]], None, None, None)).is_err());
        assert!(request(b"\x0c\x00").is_err());
    }

    #[test]
    fn answers_when_aera_does() {
        let mut fs = FileSelector::default();
        let mut effects = vec![];
        let bytes = call(OPEN, vec![], None, None, Some(false));
        let features = feature::FILE_PICKER | feature::DESKTOP_INTEGRATION;
        assert!(fs.call(&bytes, 7, feature::FILE_PICKER, &mut effects).is_some(), "base host denies picker over desktop");
        assert!(effects.is_empty());
        assert!(fs.call(&bytes, 7, features, &mut effects).is_none());
        let Some(Effect::Host(sent)) = effects.pop() else { panic!() };
        assert!(fs.call(&bytes, 8, features, &mut effects).is_some(), "one at a time");
        let mut m = Message::with(kind::FILE_PICKER_RESULT, sent.request_id, 0, 1);
        m.text = "/sdcard/a".into();
        fs.on_result(&m, &mut effects);
        let expected = standard::pigeon_success(Value::List(vec![Value::Str("/sdcard/a".into())]));
        assert_eq!(effects, vec![Effect::Respond { response: 7, bytes: expected }]);
        effects.clear();
        fs.on_result(&m, &mut effects);
        assert!(effects.is_empty(), "late result ignored");
        assert!(fs.call(&bytes, 9, features, &mut effects).is_none());
        let Some(Effect::Host(sent)) = effects.pop() else { panic!() };
        fs.on_result(&Message::with(kind::FILE_PICKER_RESULT, sent.request_id, 0, 2), &mut effects);
        assert_eq!(effects, vec![Effect::Respond { response: 9, bytes: standard::pigeon_success(Value::List(vec![])) }]);
    }
}
