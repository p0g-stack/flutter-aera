//! `file_selector`'s Linux channel
//! (`dev.flutter.pigeon.file_selector_linux.FileSelectorApi.showFileChooser`,
//! Pigeon over StandardMessageCodec). On a desktop its native half shows a
//! GTK file chooser, which flutter-aera does not load; the embedder answers
//! in its place with AERA's own picker (Host API 3 `kPickFiles`, advertised
//! as `kFeatureFilePicker`), so the stock plugin works unchanged.
//!
//! The call is answered when AERA's results arrive: every chosen path, or an
//! empty list when the user closed the picker, as GTK answers a cancel.
//! Without the feature the call fails with `unsupported`. Type groups map to
//! extensions; a group given only as MIME types shows all files, as the
//! picker has no MIME database.

use super::standard::{self, Value};
use super::Effect;
use crate::host::{feature, kind, operation, pick_mode, Message};

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
    paths: Vec<String>,
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
    let mode = match *action {
        OPEN if multiple => pick_mode::FILES,
        OPEN => pick_mode::FILE,
        CHOOSE_DIRECTORY => pick_mode::FOLDER,
        SAVE => pick_mode::SAVE,
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
            let ext = pattern.trim_start_matches('*').trim_start_matches('.').to_ascii_lowercase();
            if !ext.is_empty() && !ext.contains([',', ' ', '*', '/']) && !extensions.contains(&ext) {
                extensions.push(ext);
            }
        }
    }
    let mut m = Message::with(kind::REQUEST_OPERATION, 0, operation::PICK_FILES, mode);
    m.title = field(1).str().unwrap_or_default().to_owned();
    m.text = if mode == pick_mode::SAVE {
        field(2).str().unwrap_or_default().to_owned()
    } else if any {
        String::new()
    } else {
        extensions.join(",")
    };
    Ok(m)
}

impl FileSelector {
    /// A call from the framework. Returns the reply now, or `None` when it
    /// comes later through [`Effect::Respond`].
    pub fn call(&mut self, bytes: &[u8], response: usize, features: u32, effects: &mut Vec<Effect>) -> Option<Vec<u8>> {
        if features & feature::FILE_PICKER == 0 {
            return Some(standard::pigeon_error("unsupported", "This AERA has no file picker for plugins."));
        }
        if self.pending.is_some() {
            return Some(standard::pigeon_error("busy", "A file picker is already open."));
        }
        let mut m = match request(bytes) {
            Ok(m) => m,
            Err(e) => return Some(standard::pigeon_error("bad-args", e)),
        };
        self.next_id = self.next_id.wrapping_add(1).max(1);
        m.request_id = self.next_id;
        self.pending = Some(Pending { id: m.request_id, response, paths: vec![] });
        effects.push(Effect::Host(m));
        None
    }

    /// An `OPERATION_RESULT` from AERA.
    pub fn on_result(&mut self, m: &Message, effects: &mut Vec<Effect>) {
        let Some(p) = self.pending.as_mut().filter(|p| p.id == m.request_id) else { return };
        let bytes = if m.value == 1 {
            p.paths.push(m.text.clone());
            if m.flags & 1 != 0 {
                return;
            }
            standard::pigeon_success(Value::List(p.paths.drain(..).map(Value::Str).collect()))
        } else if m.text.is_empty() {
            standard::pigeon_success(Value::List(vec![]))
        } else {
            standard::pigeon_error("aera", &m.text)
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
    fn maps_calls_to_pick_requests() {
        let m = request(&call(OPEN, vec![vec!["*.ZIP", "*.img"], vec!["*.zip"]], Some("/sdcard/Download"), None, Some(false))).unwrap();
        assert_eq!((m.kind, m.value, m.flags), (kind::REQUEST_OPERATION, operation::PICK_FILES, pick_mode::FILE));
        assert_eq!((m.title.as_str(), m.text.as_str()), ("/sdcard/Download", "zip,img"));
        let m = request(&call(OPEN, vec![vec!["*.zip"], vec!["*"]], None, None, Some(true))).unwrap();
        assert_eq!((m.flags, m.text.as_str()), (pick_mode::FILES, ""));
        let m = request(&call(SAVE, vec![vec!["*.txt"]], None, Some("notes.txt"), None)).unwrap();
        assert_eq!((m.flags, m.text.as_str()), (pick_mode::SAVE, "notes.txt"));
        assert_eq!(request(&call(CHOOSE_DIRECTORY, vec![], None, None, None)).unwrap().flags, pick_mode::FOLDER);
        assert!(request(b"\x0c\x00").is_err());
    }

    #[test]
    fn answers_when_aera_does() {
        let mut fs = FileSelector::default();
        let mut effects = vec![];
        let bytes = call(OPEN, vec![], None, None, Some(true));
        assert!(fs.call(&bytes, 7, 0, &mut effects).is_some(), "no feature, no picker");
        assert!(fs.call(&bytes, 7, feature::FILE_PICKER, &mut effects).is_none());
        let Some(Effect::Host(sent)) = effects.pop() else { panic!() };
        assert!(fs.call(&bytes, 8, feature::FILE_PICKER, &mut effects).is_some(), "one at a time");
        let mut result = |value, flags, text: &str| {
            let mut m = Message::with(kind::OPERATION_RESULT, sent.request_id, value, flags);
            m.text = text.into();
            let mut e = vec![];
            fs.on_result(&m, &mut e);
            e
        };
        assert!(result(1, 1, "/sdcard/a").is_empty());
        let done = result(1, 0, "/sdcard/b");
        let expected = standard::pigeon_success(Value::List(vec![Value::Str("/sdcard/a".into()), Value::Str("/sdcard/b".into())]));
        assert_eq!(done, vec![Effect::Respond { response: 7, bytes: expected }]);
        assert!(result(1, 0, "/late").is_empty(), "nothing pending");
        // A closed picker is an empty list, as a GTK cancel.
        assert!(fs.call(&bytes, 9, feature::FILE_PICKER, &mut effects).is_none());
        let Some(Effect::Host(sent)) = effects.pop() else { panic!() };
        let mut e = vec![];
        fs.on_result(&Message::with(kind::OPERATION_RESULT, sent.request_id, 0, 0), &mut e);
        assert_eq!(e, vec![Effect::Respond { response: 9, bytes: standard::pigeon_success(Value::List(vec![])) }]);
    }
}
