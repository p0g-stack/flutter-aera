//! AERA's side of one Host API 2 control session
//! (`features/plugin_api/session.cpp` and `scenes/generic_plugin_scene.cpp`
//! at the pinned source checkout), plus the feature-gated recovery
//! extensions of the API 2 spike contract, without I/O.
//!
//! Modelled: the handshake and feature grant, the 128 messages/s limit,
//! setup pages as far as buttons, the back action and native text fields go,
//! the clipboard, `OPEN_DESKTOP`, `PICK_FILE` and its modal, and the
//! extensions (`SET_DESKTOP_KEYBOARD`/`DESKTOP_KEYBOARD_INSET`,
//! `EDIT_TEXT_INPUT`/`DISMISS_TEXT_INPUT`). Background/activity/rich-page
//! extensions are not advertised; unmodelled requests fail closed rather
//! than pretending an operation succeeded. `REQUEST_OPERATION` is refused
//! without its permission; `desktop-surface` is assumed granted.

use std::path::{Path, PathBuf};

use flutter_aera::host::page::{DESTRUCTIVE, DISABLED, PRIMARY};
use flutter_aera::host::text_input::MAX_CHARACTERS as MAX_INPUT_CHARACTERS;
use flutter_aera::host::{
    feature, file_picker, kind, lifecycle, text_input, Message, DEFAULT_DESKTOP, DESKTOP_SIZES,
    FEATURE_REQUEST_MAGIC, OPTIONAL_FEATURES, PROTOCOL_VERSION,
};

use crate::clipboard::Clipboard;
use crate::desktop::{Desktop, KeyboardMode, Touch};

/// Upstream `kMaxButtons`, `kMaxTextInputs`.
const MAX_BUTTONS: usize = 12;
const MAX_TEXT_INPUTS: usize = 16;
/// Upstream `Session::Poll` closes a plugin past this many packets a second.
const RATE_LIMIT: u32 = 128;
/// How long the simulated user takes to answer a picker or editor fixture.
pub const MODAL_ANSWER_MS: u64 = 300;
/// The extensions this spike adds; `--base-api2` withholds them.
pub const EXTENSIONS: u32 = feature::DESKTOP_INTEGRATION | feature::DESKTOP_TEXT_INPUT;

/// What the I/O side must do.
#[derive(Debug, PartialEq, Eq)]
pub enum Out {
    Control(Message),
    /// Allocate the frame memfd and input channel, send `DESKTOP_READY`
    /// with both, then call [`Session::desktop_opened`] (or
    /// [`Session::desktop_failed`]).
    OpenDesktop { request_id: u32, width: u32, height: u32 },
    /// Send what is left in the desktop's queue, then drop the channel and
    /// mapping.
    CloseDesktop,
    /// AERA closed the control socket.
    Disconnect,
}

pub struct Config {
    /// Optional features this host grants when asked.
    pub optional: u32,
    /// The file the simulated user picks in AERA's picker.
    pub pick_file: Option<PathBuf>,
    /// What the simulated user submits in AERA's native editor.
    pub edit_text: Option<String>,
}

impl Config {
    pub fn new(base_only: bool) -> Config {
        let supported = feature::DESKTOP | feature::DESKTOP_RESOLUTION | feature::CLIPBOARD
            | feature::TEXT_INPUT | feature::FILE_PICKER;
        let optional = supported | if base_only { 0 } else { EXTENSIONS };
        Config { optional, pick_file: None, edit_text: None }
    }
}

#[derive(Clone, Debug)]
struct TextField {
    id: u32,
    flags: u32,
    maximum: u32,
    value: String,
}

struct Picker {
    id: u32,
    folders: bool,
    extensions: Vec<String>,
    due: Option<u64>,
}

struct Editor {
    id: u32,
    due: Option<u64>,
}

pub struct Session {
    config: Config,
    negotiated: bool,
    features: u32,
    /// Why AERA closed the session, once it did.
    pub failed: Option<String>,
    /// The plugin sent `CLOSE`.
    pub closed: bool,
    rate_window: u64,
    rate_count: u32,
    page_pending: bool,
    buttons: Vec<(u32, u32)>,
    fields: Vec<TextField>,
    pending_back: u32,
    back_action: u32,
    page_title: String,
    clipboard: Clipboard,
    picker: Option<Picker>,
    editor: Option<Editor>,
    pub desktop: Option<Desktop>,
    pub out: Vec<Out>,
}

fn log(text: impl AsRef<str>) {
    println!("sim: {}", text.as_ref());
}

fn input_flags_valid(flags: u32) -> bool {
    let mask = text_input::PASSWORD | text_input::NUMBER | text_input::MULTILINE | text_input::DISABLED;
    flags & !mask == 0
        && (flags & text_input::MULTILINE == 0 || flags & (text_input::PASSWORD | text_input::NUMBER) == 0)
}

/// `InputTextValid` (a Rust `String` is already NUL-free UTF-8).
pub fn input_text_valid(text: &str, flags: u32, maximum: u32) -> bool {
    if maximum == 0 || maximum > MAX_INPUT_CHARACTERS || text.len() > 1023 || text.chars().count() as u32 > maximum {
        return false;
    }
    text.bytes().all(|c| {
        !(c < 0x20 && !(c == b'\n' && flags & text_input::MULTILINE != 0))
            && c != 0x7f
            && (flags & text_input::NUMBER == 0 || c.is_ascii_digit() || c == b'.' || c == b'-')
    })
}

/// `ParseFilePickerRequest`: `Ok((folders, extensions))`.
fn parse_pick(m: &Message) -> Result<(bool, Vec<String>), String> {
    if m.request_id == 0 || m.value > 1 || m.flags != 0 {
        return Err("Invalid file selection request.".into());
    }
    let folders = m.value == 1;
    if (!m.text.is_empty() && !m.text.starts_with('/')) || m.text.bytes().any(|c| c < 32 || c == 127) {
        return Err("The starting folder must be an absolute path.".into());
    }
    if folders && !m.title.is_empty() {
        return Err("Folder selection cannot have file filters.".into());
    }
    if m.title.ends_with(',') {
        return Err("Invalid file extension filter.".into());
    }
    let mut extensions = vec![];
    for ext in m.title.split(',').filter(|_| !m.title.is_empty()) {
        if ext.len() < 2
            || !ext.starts_with('.')
            || ext.len() > 32
            || extensions.len() >= 16
            || !ext[1..].bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-' || c == b'.')
        {
            return Err("Invalid file extension filter.".into());
        }
        extensions.push(ext.to_ascii_lowercase());
    }
    Ok((folders, extensions))
}

impl Session {
    pub fn new(config: Config) -> Session {
        Session {
            config,
            negotiated: false,
            features: 0,
            failed: None,
            closed: false,
            rate_window: 0,
            rate_count: 0,
            page_pending: false,
            buttons: vec![],
            fields: vec![],
            pending_back: 0,
            back_action: 0,
            page_title: String::new(),
            clipboard: Clipboard::default(),
            picker: None,
            editor: None,
            desktop: None,
            out: vec![],
        }
    }

    pub fn supports(&self, features: u32) -> bool {
        self.features & features == features
    }

    pub fn features(&self) -> u32 {
        self.features
    }

    pub fn connected(&self) -> bool {
        self.failed.is_none() && !self.closed
    }

    fn send(&mut self, m: Message) {
        if self.connected() {
            self.out.push(Out::Control(m));
        }
    }

    fn send_text(&mut self, kind: u32, request_id: u32, value: u32, flags: u32, text: &str) {
        self.send(Message { text: text.into(), ..Message::with(kind, request_id, value, flags) });
    }

    /// `Session::Fail` / `session.Close()` after an invalid message.
    pub fn fail(&mut self, reason: impl Into<String>) {
        if !self.connected() {
            return;
        }
        let reason = reason.into();
        log(format!("AERA closes the session: {reason}"));
        self.failed = Some(reason);
        self.out.push(Out::Disconnect);
    }

    /// The plugin's end went away; not a rejection by AERA.
    pub fn peer_gone(&mut self) {
        if self.connected() {
            log("the plugin's control socket closed");
            self.closed = true;
        }
    }

    fn modal(&self) -> bool {
        self.picker.is_some() || self.editor.is_some()
    }

    /// One packet from the plugin, `now` in ms.
    pub fn receive(&mut self, m: &Message, now: u64) {
        if !self.connected() {
            return;
        }
        if now.saturating_sub(self.rate_window) >= 1000 {
            self.rate_window = now;
            self.rate_count = 0;
        }
        self.rate_count += 1;
        if self.rate_count > RATE_LIMIT {
            return self.fail("The plugin exceeded the message rate limit.");
        }
        if !self.negotiated {
            if m.kind != kind::HELLO
                || m.value == 0
                || m.value > m.flags
                || m.value > PROTOCOL_VERSION
                || m.flags < PROTOCOL_VERSION
            {
                return self.fail("The plugin does not support AERA Host API 2.");
            }
            self.negotiated = true;
            let asked = if m.request_id & 0xFFFF_0000 == FEATURE_REQUEST_MAGIC { m.request_id } else { 0 };
            self.features = feature::METRICS | feature::BACK_NAVIGATION | (asked & self.config.optional);
            log(format!("HELLO asks {asked:#010x}; granting {:#x}", self.features));
            self.send(Message::with(kind::HELLO_ACK, 0, PROTOCOL_VERSION, self.features));
            self.send(Message::with(kind::LIFECYCLE, 0, lifecycle::RESUME, 0));
            return;
        }
        match m.kind {
            kind::HELLO => self.fail("The plugin repeated its handshake."),
            kind::PICK_FILE => self.pick_file(m, now),
            kind::GET_CLIPBOARD | kind::SET_CLIPBOARD | kind::CLIPBOARD_CHUNK => {
                let allowed = self.supports(feature::CLIPBOARD) && !self.modal();
                let reply = self.clipboard.handle(m, allowed);
                self.send(reply);
            }
            kind::OPEN_DESKTOP => self.open_desktop(m),
            kind::ADD_TEXT_INPUT => {
                let maximum = if m.value != 0 { m.value } else { MAX_INPUT_CHARACTERS };
                if !self.supports(feature::TEXT_INPUT)
                    || !self.page_pending
                    || m.request_id == 0
                    || m.title.is_empty()
                    || !input_flags_valid(m.flags)
                    || !input_text_valid(&m.text, m.flags, maximum)
                    || self.fields.len() >= MAX_TEXT_INPUTS
                    || self.fields.iter().any(|f| f.id == m.request_id)
                {
                    return self.fail("Plugin sent an invalid text input");
                }
                self.fields.push(TextField { id: m.request_id, flags: m.flags, maximum, value: m.text.clone() });
            }
            kind::UPDATE_TEXT_INPUT | kind::TEXT_INPUT_PLACEHOLDER => {
                let placeholder = m.kind == kind::TEXT_INPUT_PLACEHOLDER;
                let Some(field) = self.fields.iter().find(|f| f.id == m.request_id).cloned() else {
                    return self.fail("Plugin sent an invalid text input update");
                };
                let text_ok = if placeholder {
                    input_text_valid(&m.text, 0, MAX_INPUT_CHARACTERS)
                } else {
                    input_text_valid(&m.text, field.flags, field.maximum)
                };
                if !self.supports(feature::TEXT_INPUT) || m.value != 0 || m.flags != 0 || !m.title.is_empty() || !text_ok {
                    return self.fail("Plugin sent an invalid text input update");
                }
                if self.editor.as_ref().is_some_and(|e| e.id == m.request_id) {
                    return self.send_text(
                        kind::TEXT_INPUT_RESULT,
                        m.request_id,
                        0,
                        text_input::ERROR,
                        "The user is editing this field.",
                    );
                }
                if !placeholder {
                    if let Some(f) = self.fields.iter_mut().find(|f| f.id == m.request_id) {
                        f.value = m.text.clone();
                    }
                }
            }
            kind::BEGIN_PAGE => {
                self.cancel_picker(true);
                self.cancel_editor(true);
                self.page_title = m.title.clone();
                self.buttons.clear();
                self.fields.clear();
                self.pending_back = 0;
                self.page_pending = true;
            }
            kind::ADD_TILE if !self.supports(feature::RICH_PRESENTATION) || m.value > 11 => {
                self.fail("Plugin sent a tile without rich presentation")
            }
            kind::ADD_BUTTON | kind::ADD_TILE => {
                if !self.page_pending
                    || m.request_id == 0
                    || m.title.is_empty()
                    || m.flags & !(PRIMARY | DESTRUCTIVE | DISABLED) != 0
                    || self.buttons.len() >= MAX_BUTTONS
                    || self.buttons.iter().any(|(id, _)| *id == m.request_id)
                {
                    return self.fail("Plugin page exceeded the Host API limit");
                }
                self.buttons.push((m.request_id, m.flags));
            }
            kind::SET_BACK_ACTION => {
                if !self.page_pending
                    || m.request_id == 0
                    || m.value != 0
                    || m.flags != 0
                    || !m.title.is_empty()
                    || !m.text.is_empty()
                {
                    return self.fail("Plugin sent an invalid back action");
                }
                self.pending_back = m.request_id;
            }
            kind::COMMIT_PAGE => {
                let back = self.pending_back;
                if !self.page_pending
                    || (back != 0 && !self.buttons.iter().any(|(id, f)| *id == back && f & DISABLED == 0))
                {
                    return self.fail("Plugin sent an invalid Host API sequence");
                }
                self.page_pending = false;
                self.back_action = back;
                log(format!(
                    "page {:?}: buttons {:?}, text fields {:?}, back {}",
                    self.page_title,
                    self.buttons.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                    self.fields.iter().map(|f| f.id).collect::<Vec<_>>(),
                    back
                ));
            }
            kind::SET_STATUS => log(format!("status {:?}", m.text)),
            kind::REQUEST_OPERATION => self.send_text(
                kind::OPERATION_RESULT,
                m.request_id,
                0,
                0,
                "Permission denied by the AERA host.",
            ),
            kind::CLOSE => {
                self.cancel_picker(false);
                self.cancel_editor(false);
                log("plugin closed");
                self.closed = true;
                self.out.push(Out::Disconnect);
            }
            kind::SET_DESKTOP_KEYBOARD => self.set_desktop_keyboard(m),
            kind::EDIT_TEXT_INPUT => self.edit_text_input(m, now),
            kind::DISMISS_TEXT_INPUT => self.dismiss_text_input(m),
            k if k > kind::HELLO && k < kind::OPEN_DESKTOP => self.fail(format!("Simulator does not model control kind {k}")),
            k => self.fail(format!("Plugin sent an invalid Host API sequence (kind {k})")),
        }
    }

    fn pick_file(&mut self, m: &Message, now: u64) {
        let desktop_ok = self.desktop.is_none() || self.supports(feature::DESKTOP_INTEGRATION);
        let allowed = self.supports(feature::FILE_PICKER) && !self.modal() && desktop_ok && !self.page_pending;
        let parsed = if allowed { parse_pick(m) } else { Err(String::new()) };
        match parsed {
            Err(error) => {
                let error = if error.is_empty() { "File selection is not available now.".into() } else { error };
                log(format!("PICK_FILE {} refused: {error}", m.request_id));
                self.send_text(kind::FILE_PICKER_RESULT, m.request_id, 0, file_picker::ERROR, &error);
            }
            Ok((folders, extensions)) => {
                log(format!(
                    "picker {} opens ({}, filters {extensions:?}, start {:?})",
                    m.request_id,
                    if folders { "folder" } else { "file" },
                    m.text
                ));
                // The modal takes the input: held desktop input is let go.
                if let Some(d) = &mut self.desktop {
                    d.release();
                    d.hide_keyboard();
                }
                self.keyboard_changed();
                let due = self.config.pick_file.is_some().then_some(now + MODAL_ANSWER_MS);
                self.picker = Some(Picker { id: m.request_id, folders, extensions, due });
            }
        }
    }

    fn open_desktop(&mut self, m: &Message) {
        let (mut width, mut height) = (m.value, m.flags);
        if width == 0 && height == 0 {
            (width, height) = DEFAULT_DESKTOP;
        }
        if !self.supports(feature::DESKTOP)
            || m.request_id == 0
            || self.desktop.is_some()
            || self.editor.is_some()
            || self.page_pending
            || !DESKTOP_SIZES.contains(&(width, height))
        {
            log(format!("OPEN_DESKTOP {} {width}x{height} refused", m.request_id));
            return self.send_text(
                kind::DESKTOP_READY,
                m.request_id,
                0,
                0,
                "Desktop request unavailable or not permitted.",
            );
        }
        if self.connected() {
            self.out.push(Out::OpenDesktop { request_id: m.request_id, width, height });
        }
    }

    /// `DESKTOP_READY` went out with both descriptors.
    pub fn desktop_opened(&mut self, width: u32, height: u32) {
        log(format!("desktop {width}x{height} open"));
        self.desktop = Some(Desktop::new(width, height));
        if self.supports(feature::DESKTOP_INTEGRATION) {
            self.send(Message::with(kind::DESKTOP_KEYBOARD_INSET, 0, 0, 0));
        }
    }

    pub fn desktop_failed(&mut self, request_id: u32) {
        self.send_text(kind::DESKTOP_READY, request_id, 0, 0, "Could not allocate desktop surface.");
    }

    /// EXIT was released or the run ends: the desktop's last messages
    /// (`RELEASE`, `CLOSE`) are queued; the I/O side sends them on
    /// [`Out::CloseDesktop`] and then takes [`Session::desktop`].
    pub fn close_desktop(&mut self) {
        if let Some(d) = &mut self.desktop {
            d.close();
            self.out.push(Out::CloseDesktop);
        }
    }

    fn set_desktop_keyboard(&mut self, m: &Message) {
        if !self.supports(feature::DESKTOP_INTEGRATION) {
            return self.fail("SET_DESKTOP_KEYBOARD without desktop integration");
        }
        let mode = match m.flags {
            0 => KeyboardMode::Text,
            f if f == text_input::NUMBER => KeyboardMode::Number,
            f if f == text_input::MULTILINE => KeyboardMode::Multiline,
            _ => return self.fail("Plugin sent an invalid desktop keyboard request"),
        };
        if m.request_id == 0 || m.value > 1 || !m.title.is_empty() || !m.text.is_empty() {
            return self.fail("Plugin sent an invalid desktop keyboard request");
        }
        if self.modal() {
            return log("SET_DESKTOP_KEYBOARD ignored under a modal");
        }
        let Some(d) = &mut self.desktop else {
            return log("SET_DESKTOP_KEYBOARD ignored: no desktop");
        };
        if m.value == 1 {
            if d.keyboard().is_none() {
                d.release();
            }
            d.show_keyboard(mode);
        } else {
            d.hide_keyboard();
        }
        self.keyboard_changed();
    }

    /// Reports a shown or hidden keyboard, extension only.
    fn keyboard_changed(&mut self) {
        let Some(d) = &mut self.desktop else { return };
        let Some(shown) = d.take_keyboard_change() else { return };
        let inset = d.keyboard_inset();
        log(format!("keyboard {} (inset {inset})", if shown { "shown" } else { "hidden" }));
        if self.supports(feature::DESKTOP_INTEGRATION) {
            self.send(Message::with(kind::DESKTOP_KEYBOARD_INSET, 0, inset, 0));
        }
    }

    fn edit_text_input(&mut self, m: &Message, now: u64) {
        if !self.supports(feature::DESKTOP_TEXT_INPUT) {
            return self.fail("EDIT_TEXT_INPUT without desktop text input");
        }
        if m.request_id == 0 || m.value != 0 || m.flags != 0 || !m.title.is_empty() || !m.text.is_empty() {
            return self.fail("Plugin sent an invalid text input edit");
        }
        if self.editor.as_ref().is_some_and(|e| e.id == m.request_id) {
            return;
        }
        let field = self.fields.iter().find(|f| f.id == m.request_id);
        let refusal = match field {
            None => Some("No such text field."),
            Some(f) if f.flags & text_input::DISABLED != 0 => Some("The text field is disabled."),
            Some(_) if self.page_pending || self.modal() => Some("Text editing is not available now."),
            Some(_) => None,
        };
        if let Some(refusal) = refusal {
            log(format!("EDIT_TEXT_INPUT {} refused: {refusal}", m.request_id));
            return self.send_text(kind::TEXT_INPUT_RESULT, m.request_id, 0, text_input::ERROR, refusal);
        }
        log(format!("native editor opens for field {}", m.request_id));
        if let Some(d) = &mut self.desktop {
            d.release();
            d.hide_keyboard();
        }
        self.keyboard_changed();
        let due = self.config.edit_text.is_some().then_some(now + MODAL_ANSWER_MS);
        self.editor = Some(Editor { id: m.request_id, due });
    }

    fn dismiss_text_input(&mut self, m: &Message) {
        if !self.supports(feature::DESKTOP_TEXT_INPUT) {
            return self.fail("DISMISS_TEXT_INPUT without desktop text input");
        }
        if m.request_id == 0 || m.value != 0 || m.flags != 0 || !m.title.is_empty() || !m.text.is_empty() {
            return self.fail("Plugin sent an invalid text input dismissal");
        }
        if self.editor.as_ref().is_some_and(|e| e.id == m.request_id) {
            self.cancel_editor(true);
        }
    }

    fn cancel_picker(&mut self, notify: bool) {
        if let Some(p) = self.picker.take() {
            log(format!("picker {} cancelled", p.id));
            if notify {
                self.send(Message::with(kind::FILE_PICKER_RESULT, p.id, 0, file_picker::CANCELLED));
            }
        }
    }

    fn cancel_editor(&mut self, notify: bool) {
        if let Some(e) = self.editor.take() {
            log(format!("editor for field {} cancelled", e.id));
            if notify {
                self.send(Message::with(kind::TEXT_INPUT_RESULT, e.id, 0, text_input::CANCELLED));
            }
        }
    }

    /// Answers due fixtures: the simulated user picking or submitting.
    pub fn poll(&mut self, now: u64) {
        if self.picker.as_ref().and_then(|p| p.due).is_some_and(|due| now >= due) {
            let p = self.picker.take().unwrap();
            let path = self.config.pick_file.clone().unwrap();
            match fixture_matches(&path, p.folders, &p.extensions) {
                Ok(()) => {
                    log(format!("picker {} selects {}", p.id, path.display()));
                    self.send_text(
                        kind::FILE_PICKER_RESULT,
                        p.id,
                        0,
                        file_picker::SELECTED,
                        &path.to_string_lossy(),
                    );
                }
                Err(why) => {
                    log(format!("picker {}: the fixture {} cannot be picked ({why}); cancelling", p.id, path.display()));
                    self.send(Message::with(kind::FILE_PICKER_RESULT, p.id, 0, file_picker::CANCELLED));
                }
            }
        }
        if self.editor.as_ref().and_then(|e| e.due).is_some_and(|due| now >= due) {
            let id = self.editor.as_ref().unwrap().id;
            let text = self.config.edit_text.clone().unwrap();
            let field = self.fields.iter().find(|f| f.id == id).cloned();
            match field {
                Some(f) if input_text_valid(&text, f.flags, f.maximum) => {
                    self.editor = None;
                    log(format!("editor for field {id} submits {text:?}"));
                    if let Some(f) = self.fields.iter_mut().find(|f| f.id == id) {
                        f.value = text.clone();
                    }
                    let chars = text.chars().count() as u32;
                    self.send_text(kind::TEXT_INPUT_RESULT, id, chars, text_input::SUBMITTED, &text);
                }
                _ => {
                    // AERA's Done does nothing for text the field rejects.
                    log(format!("editor for field {id} rejects {text:?}; it stays open until --back"));
                    if let Some(e) = &mut self.editor {
                        e.due = None;
                    }
                }
            }
        }
    }

    /// A scripted touch, in desktop pixels.
    pub fn tap(&mut self, x: i32, y: i32, now: u64) {
        if self.modal() {
            return log("tap taken by AERA's modal");
        }
        let Some(d) = &mut self.desktop else {
            return log("tap on the setup page (not modelled; use --action ID)");
        };
        if d.tap(x, y, now) == Touch::Keyboard {
            log("tap taken by AERA's keyboard");
        }
        self.keyboard_changed();
    }

    /// Scripted typing on AERA's on-screen keyboard.
    pub fn type_text(&mut self, text: &str) {
        if self.modal() {
            return log("typing taken by AERA's modal");
        }
        let unicode = self.supports(feature::DESKTOP_INTEGRATION);
        let Some(d) = &mut self.desktop else {
            return log("typing without a desktop (not modelled)");
        };
        for c in text.chars() {
            if let Err(why) = d.type_char(c, unicode) {
                log(format!("not typed: {why}"));
            }
        }
        self.keyboard_changed();
    }

    /// The user goes back: a modal cancels; on the desktop that is the ESC
    /// toolbar key; on a page, its back action.
    pub fn back(&mut self, now: u64) {
        if self.picker.is_some() {
            return self.cancel_picker(true);
        }
        if self.editor.is_some() {
            return self.cancel_editor(true);
        }
        if let Some(d) = &self.desktop {
            // The ESC toolbar key's centre in AERA's virtual layout.
            let (x, y) = d.from_virtual(240, 32);
            return self.tap(x, y, now);
        }
        if self.back_action != 0 {
            let action = self.back_action;
            self.action(action);
        } else {
            log("back: the page has no back action");
        }
    }

    /// The user presses page button `id`.
    pub fn action(&mut self, id: u32) {
        let enabled = self.buttons.iter().any(|(b, f)| *b == id && f & DISABLED == 0);
        if self.page_pending || self.modal() || self.desktop.is_some() || !enabled {
            return log(format!("button {id} is not on screen"));
        }
        self.back_action = 0;
        self.send(Message::with(kind::ACTION, id, 0, 0));
    }

    /// The scene goes away: `LIFECYCLE` `STOP`.
    pub fn stop(&mut self) {
        self.cancel_picker(false);
        self.cancel_editor(false);
        if self.negotiated {
            self.send(Message::with(kind::LIFECYCLE, 0, lifecycle::STOP, 0));
        }
    }
}

/// Whether the simulated user could pick `path` in this picker.
fn fixture_matches(path: &Path, folders: bool, extensions: &[String]) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if folders != meta.is_dir() {
        return Err(if folders { "not a folder".into() } else { "not a file".into() });
    }
    if !folders && !extensions.is_empty() {
        let name = path.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if !extensions.iter().any(|e| name.ends_with(e.as_str())) {
            return Err(format!("not one of {extensions:?}"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flutter_aera::host::play;

    fn hello(features: u32) -> Message {
        Message::with(kind::HELLO, FEATURE_REQUEST_MAGIC | features, PROTOCOL_VERSION, PROTOCOL_VERSION)
    }

    fn control(s: &mut Session) -> Vec<Message> {
        s.out
            .drain(..)
            .filter_map(|o| match o {
                Out::Control(m) => Some(m),
                _ => None,
            })
            .collect()
    }

    fn session(base: bool, asked: u32) -> Session {
        let mut s = Session::new(Config::new(base));
        s.receive(&hello(asked), 0);
        s.out.clear();
        s
    }

    fn with_desktop(base: bool) -> Session {
        let mut s = session(base, OPTIONAL_FEATURES);
        s.receive(&Message::with(kind::OPEN_DESKTOP, 9, 0, 0), 0);
        assert_eq!(s.out, vec![Out::OpenDesktop { request_id: 9, width: 960, height: 540 }]);
        s.out.clear();
        s.desktop_opened(960, 540);
        s
    }

    fn page_with_field(s: &mut Session, flags: u32) {
        s.receive(&Message::new(kind::BEGIN_PAGE), 0);
        s.receive(&Message { title: "Name".into(), ..Message::with(kind::ADD_TEXT_INPUT, 3, 0, flags) }, 0);
        s.receive(&Message::new(kind::COMMIT_PAGE), 0);
    }

    #[test]
    fn hello_grants_what_was_asked_and_this_host_has() {
        let mut s = Session::new(Config::new(false));
        s.receive(&hello(feature::DESKTOP | EXTENSIONS), 0);
        let m = control(&mut s);
        assert_eq!(m[0].kind, kind::HELLO_ACK);
        assert_eq!(m[0].value, 2);
        assert_eq!(m[0].flags, feature::METRICS | feature::BACK_NAVIGATION | feature::DESKTOP | EXTENSIONS);
        assert_eq!((m[1].kind, m[1].value), (kind::LIFECYCLE, lifecycle::RESUME));

        let mut s = Session::new(Config::new(true));
        s.receive(&hello(feature::DESKTOP | EXTENSIONS), 0);
        assert_eq!(control(&mut s)[0].flags, feature::METRICS | feature::BACK_NAVIGATION | feature::DESKTOP);

        // No marker: legacy request IDs are not features.
        let mut s = Session::new(Config::new(false));
        s.receive(&Message::with(kind::HELLO, feature::DESKTOP, 2, 2), 0);
        assert_eq!(control(&mut s)[0].flags, feature::METRICS | feature::BACK_NAVIGATION);
    }

    #[test]
    fn bad_or_repeated_hello_and_floods_close_the_session() {
        let mut s = Session::new(Config::new(false));
        s.receive(&Message::with(kind::HELLO, 0, 3, 3), 0);
        assert!(s.failed.is_some());
        let mut s = session(false, 0);
        s.receive(&hello(0), 0);
        assert!(s.failed.is_some());
        let mut s = session(false, 0);
        for _ in 0..RATE_LIMIT {
            s.receive(&Message::new(kind::SET_STATUS), 1000);
        }
        assert!(s.failed.is_none());
        s.receive(&Message::new(kind::SET_STATUS), 1000);
        assert!(s.failed.is_some());
    }

    #[test]
    fn open_desktop_defaults_and_refusals() {
        let mut s = session(false, feature::DESKTOP);
        s.receive(&Message::with(kind::OPEN_DESKTOP, 4, 1280, 720), 0);
        assert_eq!(s.out, vec![Out::OpenDesktop { request_id: 4, width: 1280, height: 720 }]);
        s.out.clear();
        s.receive(&Message::with(kind::OPEN_DESKTOP, 5, 1000, 500), 0);
        let m = control(&mut s);
        assert_eq!((m[0].kind, m[0].request_id, m[0].value, m[0].flags), (kind::DESKTOP_READY, 5, 0, 0));
        assert!(!m[0].text.is_empty());
        let mut s = session(false, 0);
        s.receive(&Message::with(kind::OPEN_DESKTOP, 4, 0, 0), 0);
        assert_eq!(control(&mut s)[0].kind, kind::DESKTOP_READY);
    }

    #[test]
    fn extension_kinds_need_their_bit() {
        let mut s = with_desktop(true);
        s.out.clear();
        s.receive(&Message::with(kind::SET_DESKTOP_KEYBOARD, 1, 1, 0), 0);
        assert!(s.failed.is_some());
        let mut s = with_desktop(true);
        page_with_field(&mut s, 0);
        s.receive(&Message::with(kind::EDIT_TEXT_INPUT, 3, 0, 0), 0);
        assert!(s.failed.is_some());
    }

    #[test]
    fn base_desktop_has_no_inset_and_no_picker() {
        let mut s = with_desktop(true);
        assert!(control(&mut s).is_empty());
        // KEYS shows the keyboard; nothing is said about geometry.
        s.tap(345, 32, 0);
        assert!(s.desktop.as_ref().unwrap().keyboard().is_some());
        assert!(control(&mut s).is_empty());
        s.receive(&Message::with(kind::PICK_FILE, 7, 0, 0), 0);
        let m = control(&mut s);
        assert_eq!((m[0].kind, m[0].flags), (kind::FILE_PICKER_RESULT, file_picker::ERROR));
    }

    #[test]
    fn desktop_keyboard_extension_reports_insets() {
        let mut s = with_desktop(false);
        let m = control(&mut s);
        assert_eq!((m[0].kind, m[0].value), (kind::DESKTOP_KEYBOARD_INSET, 0));
        s.receive(&Message::with(kind::SET_DESKTOP_KEYBOARD, 1, 1, text_input::MULTILINE), 0);
        assert_eq!(s.desktop.as_ref().unwrap().out, vec![play::PlayMessage::new(play::RELEASE, 0, 0, 0)]);
        let m = control(&mut s);
        assert_eq!((m[0].kind, m[0].request_id, m[0].value, m[0].flags), (kind::DESKTOP_KEYBOARD_INSET, 0, 297, 0));
        // Showing again changes nothing on the wire.
        s.receive(&Message::with(kind::SET_DESKTOP_KEYBOARD, 2, 1, 0), 0);
        assert!(control(&mut s).is_empty());
        // The user's OK hides it.
        s.type_text("\n");
        assert_eq!(control(&mut s)[0].value, 0);
        // Combined flags are malformed.
        s.receive(&Message::with(kind::SET_DESKTOP_KEYBOARD, 3, 1, text_input::MULTILINE | text_input::NUMBER), 0);
        assert!(s.failed.is_some());
    }

    #[test]
    fn picker_over_the_desktop_suspends_input_until_answered() {
        let mut s = with_desktop(false);
        s.config.pick_file = Some(std::env::temp_dir());
        control(&mut s);
        s.receive(&Message::with(kind::PICK_FILE, 7, 1, 0), 100);
        assert!(control(&mut s).is_empty());
        let d = s.desktop.as_mut().unwrap();
        assert_eq!(d.out.drain(..).map(|p| p.kind).collect::<Vec<_>>(), vec![play::RELEASE]);
        // Input is the modal's; a second modal is refused.
        s.tap(700, 300, 150);
        assert!(s.desktop.as_ref().unwrap().out.is_empty());
        s.receive(&Message::with(kind::PICK_FILE, 8, 0, 0), 150);
        assert_eq!(control(&mut s)[0].flags, file_picker::ERROR);
        s.poll(100 + MODAL_ANSWER_MS);
        let m = control(&mut s);
        assert_eq!((m[0].kind, m[0].request_id, m[0].flags), (kind::FILE_PICKER_RESULT, 7, file_picker::SELECTED));
        assert_eq!(m[0].text, std::env::temp_dir().to_string_lossy());
        s.tap(700, 300, 500);
        assert!(!s.desktop.as_ref().unwrap().out.is_empty());
    }

    #[test]
    fn picker_requests_are_parsed_as_aera_does() {
        let pick = |value, title: &str, text: &str| {
            parse_pick(&Message { title: title.into(), text: text.into(), ..Message::with(kind::PICK_FILE, 1, value, 0) })
        };
        assert_eq!(pick(0, ".ZIP,.img", "/sdcard"), Ok((false, vec![".zip".into(), ".img".into()])));
        assert!(pick(2, "", "").is_err()); // several files: unsupported
        assert!(pick(1, ".zip", "").is_err());
        assert!(pick(0, ".zip,", "").is_err());
        assert!(pick(0, "zip", "").is_err());
        assert!(pick(0, "", "relative").is_err());
    }

    #[test]
    fn back_cancels_the_picker() {
        let mut s = session(false, feature::FILE_PICKER);
        s.receive(&Message::with(kind::PICK_FILE, 7, 0, 0), 0);
        s.poll(10_000); // no fixture: the user has not answered
        assert!(control(&mut s).is_empty());
        s.back(0);
        assert_eq!(control(&mut s)[0].flags, file_picker::CANCELLED);
    }

    #[test]
    fn native_editor_over_the_desktop() {
        let mut s = with_desktop(false);
        s.config.edit_text = Some("héllo".into());
        page_with_field(&mut s, 0);
        control(&mut s);
        s.receive(&Message::with(kind::EDIT_TEXT_INPUT, 4, 0, 0), 0);
        assert_eq!(control(&mut s)[0].flags, text_input::ERROR);
        s.receive(&Message::with(kind::EDIT_TEXT_INPUT, 3, 0, 0), 0);
        assert!(control(&mut s).is_empty());
        s.receive(&Message { text: "x".into(), ..Message::with(kind::UPDATE_TEXT_INPUT, 3, 0, 0) }, 0);
        assert_eq!(control(&mut s)[0].text, "The user is editing this field.");
        s.poll(MODAL_ANSWER_MS);
        let m = control(&mut s);
        assert_eq!((m[0].kind, m[0].request_id, m[0].value, m[0].flags), (kind::TEXT_INPUT_RESULT, 3, 5, text_input::SUBMITTED));
        assert_eq!(m[0].text, "héllo");

        s.config.edit_text = None;
        s.receive(&Message::with(kind::EDIT_TEXT_INPUT, 3, 0, 0), 0);
        s.receive(&Message::with(kind::DISMISS_TEXT_INPUT, 3, 0, 0), 0);
        assert_eq!(control(&mut s)[0].flags, text_input::CANCELLED);
        s.receive(&Message::with(kind::DISMISS_TEXT_INPUT, 3, 0, 0), 0);
        assert!(control(&mut s).is_empty(), "dismissal without an editor is idempotent");
    }

    #[test]
    fn a_number_field_keeps_its_editor_open_on_letters() {
        let mut s = session(false, OPTIONAL_FEATURES);
        s.config.edit_text = Some("abc".into());
        page_with_field(&mut s, text_input::NUMBER);
        s.receive(&Message::with(kind::EDIT_TEXT_INPUT, 3, 0, 0), 0);
        s.poll(MODAL_ANSWER_MS);
        assert!(control(&mut s).is_empty());
        s.back(0);
        assert_eq!(control(&mut s)[0].flags, text_input::CANCELLED);
    }

    #[test]
    fn page_buttons_and_back() {
        let mut s = session(false, 0);
        s.receive(&Message::new(kind::BEGIN_PAGE), 0);
        s.receive(&Message { title: "Open".into(), ..Message::with(kind::ADD_BUTTON, 1, 0, PRIMARY) }, 0);
        s.receive(&Message::with(kind::SET_BACK_ACTION, 1, 0, 0), 0);
        s.action(1);
        assert!(control(&mut s).is_empty()); // not committed yet
        s.receive(&Message::new(kind::COMMIT_PAGE), 0);
        s.back(0);
        let m = control(&mut s);
        assert_eq!((m[0].kind, m[0].request_id), (kind::ACTION, 1));
    }
}
