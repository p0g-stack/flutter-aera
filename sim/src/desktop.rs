//! AERA's plugin desktop (`aeraui/components/plugin_desktop.cpp` at the
//! pinned source checkout), without LVGL: the 16 ms frame timer, the touch
//! toolbar and game controls, and the on-screen keyboard. Every
//! `PlayMessage` it queues is one AERA would send on the input channel;
//! nothing here does I/O.
//!
//! The simulated screen is the desktop frame itself (AERA letterboxes the
//! frame onto its landscape panel at whatever scale fits; the simulator's
//! panel is the frame, scale 1), so a touch at desktop pixel (x, y) maps to
//! AERA's fixed 960x540 control layout exactly as AERA maps panel pixels.

use flutter_aera::host::input::keysym::{BACKSPACE, ESCAPE, RETURN, TAB, UNICODE};
use flutter_aera::host::play;
use flutter_aera::host::PlayMessage as Play;

/// AERA's fixed control layout; only guest pointer coordinates scale.
pub const VIRTUAL_WIDTH: i32 = 960;
pub const VIRTUAL_HEIGHT: i32 = 540;
/// The top toolbar (EXIT MENU ESC KEYS ENTER TAB) takes touches here, in
/// the virtual layout.
pub const TOOLBAR_RIGHT: i32 = 580;
pub const TOOLBAR_BOTTOM: i32 = 65;
/// `lv_obj_set_size(keyboard, sw, sh * 55 / 100)`, bottom-aligned.
pub const KEYBOARD_PERCENT: u32 = 55;
/// A play-mode tap must lift within this many ms and 12 virtual pixels.
const TAP_MS: u64 = 350;
const TAP_SLOP: i32 = 12;

/// X11 `Shift_L` and `Control_L`, which only the host sends.
pub const SHIFT_L: u32 = 0xffe1;
pub const CONTROL_L: u32 = 0xffe3;

/// Which map AERA's keyboard shows. The base toolbar KEYS button always
/// shows `Text`; the others come only from `SET_DESKTOP_KEYBOARD`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardMode {
    /// Single line: the bottom-right key is OK, which hides the keyboard.
    Text,
    /// LVGL's number map: digits, `.`, backspace, OK.
    Number,
    /// The OK key is replaced by new line, which sends Return.
    Multiline,
}

/// What became of one touch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Touch {
    /// The desktop took it.
    Desktop,
    /// AERA's keyboard is up; the touch went to LVGL, not the app.
    Keyboard,
}

/// What one frame-timer tick does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    /// EXIT was released: tear the desktop down.
    Close,
    /// Read at most one `PlayMessage` and hand it to [`Desktop::frame`].
    Receive,
    /// The input channel is closed; nothing to read.
    Idle,
}

#[derive(Clone, Copy, Debug, Default)]
struct Contact {
    down: bool,
    role: u8,
    x: i32,
    y: i32,
    start_x: i32,
    start_y: i32,
    started_ms: u64,
    dragged: bool,
}

pub struct Desktop {
    pub width: u32,
    pub height: u32,
    /// The last frame received; the next must be `sequence + 1`.
    sequence: u32,
    ack: bool,
    menu: bool,
    exit: bool,
    connected: bool,
    contact: Contact,
    movement: [bool; 4],
    keyboard: Option<KeyboardMode>,
    keyboard_changed: bool,
    /// Messages for the input channel, in order.
    pub out: Vec<Play>,
}

impl Desktop {
    pub fn new(width: u32, height: u32) -> Desktop {
        Desktop {
            width,
            height,
            sequence: 0,
            ack: false,
            menu: true,
            exit: false,
            connected: true,
            contact: Contact::default(),
            movement: [false; 4],
            keyboard: None,
            keyboard_changed: false,
            out: vec![],
        }
    }

    pub fn frame_bytes(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }

    /// The slot on screen (`sequence % 2`), once a frame arrived.
    pub fn latched_slot(&self) -> Option<usize> {
        (self.sequence > 0).then_some(self.sequence as usize % 2)
    }

    pub fn sequence(&self) -> u32 {
        self.sequence
    }

    pub fn connected(&self) -> bool {
        self.connected
    }

    pub fn menu(&self) -> bool {
        self.menu
    }

    pub fn keyboard(&self) -> Option<KeyboardMode> {
        self.keyboard
    }

    /// Bottom occlusion of the keyboard in desktop pixels: on AERA's panel
    /// the keyboard is 55 % of the screen height; with the frame as the
    /// panel that is 55 % of the frame height.
    pub fn keyboard_inset(&self) -> u32 {
        if self.keyboard.is_some() {
            self.height * KEYBOARD_PERCENT / 100
        } else {
            0
        }
    }

    /// `Some(shown)` once after the keyboard was shown or hidden.
    pub fn take_keyboard_change(&mut self) -> Option<bool> {
        std::mem::take(&mut self.keyboard_changed).then_some(self.keyboard.is_some())
    }

    fn send(&mut self, kind: u32, x: i32, y: i32) {
        if !self.connected {
            return;
        }
        let (x, y) = if kind == play::POINTER {
            (x * self.width as i32 / VIRTUAL_WIDTH, y * self.height as i32 / VIRTUAL_HEIGHT)
        } else {
            (x, y)
        };
        self.out.push(Play { kind, sequence: self.sequence, x, y });
    }

    /// A send failed: AERA marks the channel broken and closes it.
    pub fn disconnect(&mut self) {
        self.connected = false;
    }

    /// `PlayScene::Release`: cancels held buttons, keys and movement.
    pub fn release(&mut self) {
        self.send(play::RELEASE, 0, 0);
        self.contact = Contact::default();
        self.movement = [false; 4];
    }

    /// `~PlayScene`: what the app hears when the desktop goes away.
    pub fn close(&mut self) {
        self.send(play::RELEASE, 0, 0);
        self.send(play::CLOSE, 0, 0);
        self.connected = false;
    }

    /// One run of the 16 ms timer: the pending ACK goes out before the next
    /// frame is read, so a frame is acknowledged one tick after it was
    /// latched.
    pub fn tick(&mut self) -> Tick {
        if self.exit {
            return Tick::Close;
        }
        if self.ack {
            self.send(play::ACK, 0, 0);
            self.ack = false;
        }
        if self.connected {
            Tick::Receive
        } else {
            Tick::Idle
        }
    }

    /// One received `PlayMessage`: only the exact next `FRAME` of the
    /// negotiated size is accepted; anything else closes the channel, as
    /// AERA does ("Application stopped"). `Ok` is the slot now latched.
    pub fn frame(&mut self, m: Play) -> Result<usize, String> {
        let problem = if m.kind != play::FRAME {
            Some(format!("kind {} where only FRAME is valid", m.kind))
        } else if m.x != self.width as i32 || m.y != self.height as i32 {
            Some(format!("frame {}x{} on a {}x{} desktop", m.x, m.y, self.width, self.height))
        } else if m.sequence != self.sequence.wrapping_add(1) {
            Some(format!("frame {} after {}", m.sequence, self.sequence))
        } else {
            None
        };
        if let Some(problem) = problem {
            self.connected = false;
            return Err(problem);
        }
        self.sequence = m.sequence;
        self.ack = true;
        Ok(self.sequence as usize % 2)
    }

    fn to_virtual(&self, px: i32, py: i32) -> (i32, i32) {
        (
            (px * VIRTUAL_WIDTH / self.width.max(1) as i32).clamp(0, VIRTUAL_WIDTH - 1),
            (py * VIRTUAL_HEIGHT / self.height.max(1) as i32).clamp(0, VIRTUAL_HEIGHT - 1),
        )
    }

    /// The desktop pixel at the centre of a virtual-layout point.
    pub fn from_virtual(&self, vx: i32, vy: i32) -> (i32, i32) {
        (vx * self.width as i32 / VIRTUAL_WIDTH, vy * self.height as i32 / VIRTUAL_HEIGHT)
    }

    /// `PluginDesktopHandlePointer` for touch slot 0, at desktop pixels.
    pub fn pointer(&mut self, down: bool, px: i32, py: i32, now_ms: u64) -> Touch {
        if self.keyboard.is_some() {
            return Touch::Keyboard;
        }
        let (x, y) = self.to_virtual(px, py);
        let toolbar = y < TOOLBAR_BOTTOM;
        if down && !self.contact.down {
            self.contact = Contact {
                down: true,
                role: 0,
                x,
                y,
                start_x: x,
                start_y: y,
                started_ms: now_ms,
                dragged: false,
            };
            let role = if toolbar && x < 75 {
                Some(10) // EXIT, on release
            } else if toolbar && x < 190 {
                Some(11) // MENU/PLAY, on release
            } else if toolbar && x < 290 {
                self.send(play::KEY, ESCAPE as i32, 1);
                Some(12)
            } else if toolbar && x < 400 {
                Some(14) // KEYS, on release
            } else if toolbar && x < 490 {
                self.send(play::KEY, RETURN as i32, 1);
                Some(15)
            } else if toolbar && x < TOOLBAR_RIGHT {
                self.send(play::KEY, TAB as i32, 1);
                Some(16)
            } else if self.menu {
                self.send(play::POINTER, x, y);
                self.send(play::BUTTON, 1, 1);
                Some(1)
            } else {
                None
            };
            if let Some(role) = role {
                self.contact.role = role;
                return Touch::Desktop;
            }
            self.contact.role = if x < 280 && y > 240 {
                2
            } else if x > 850 && y > 440 {
                self.send(play::BUTTON, 1, 1);
                3
            } else if x > 850 && y > 350 {
                self.send(play::KEY, ' ' as i32, 1);
                4
            } else if x > 850 && y > 260 {
                self.send(play::KEY, 'r' as i32, 1);
                5
            } else if x > 750 && y > 440 {
                self.send(play::KEY, 'f' as i32, 1);
                6
            } else if x > 650 && y > 440 {
                self.send(play::KEY, CONTROL_L as i32, 1);
                7
            } else if x > 550 && y > 440 {
                self.send(play::BUTTON, 3, 1);
                8
            } else if x < 400 && y > 440 {
                self.send(play::KEY, 'v' as i32, 1);
                9
            } else {
                13
            };
        }
        if !down && self.contact.down {
            let c = self.contact;
            match c.role {
                1 | 3 => self.send(play::BUTTON, 1, 0),
                2 => {
                    for (i, key) in "wasd".chars().enumerate() {
                        self.send(play::KEY, key as i32, 0);
                        self.movement[i] = false;
                    }
                }
                4 => self.send(play::KEY, ' ' as i32, 0),
                5 => self.send(play::KEY, 'r' as i32, 0),
                6 => self.send(play::KEY, 'f' as i32, 0),
                7 => self.send(play::KEY, CONTROL_L as i32, 0),
                8 => self.send(play::BUTTON, 3, 0),
                9 => self.send(play::KEY, 'v' as i32, 0),
                10 if toolbar && x < 75 => self.exit = true,
                11 if toolbar && (75..190).contains(&x) => {
                    self.release();
                    self.menu = !self.menu;
                }
                12 => self.send(play::KEY, ESCAPE as i32, 0),
                14 if toolbar && (290..400).contains(&x) => {
                    self.release();
                    self.show_keyboard(KeyboardMode::Text);
                }
                15 => self.send(play::KEY, RETURN as i32, 0),
                16 => self.send(play::KEY, TAB as i32, 0),
                13 if !c.dragged
                    && now_ms.saturating_sub(c.started_ms) < TAP_MS
                    && (x - c.start_x).abs() <= TAP_SLOP
                    && (y - c.start_y).abs() <= TAP_SLOP =>
                {
                    self.send(play::BUTTON, 1, 1);
                    self.send(play::BUTTON, 1, 0);
                }
                _ => {}
            }
            self.contact = Contact::default();
            return Touch::Desktop;
        }
        if down {
            let c = &mut self.contact;
            if (x - c.start_x).abs() > TAP_SLOP || (y - c.start_y).abs() > TAP_SLOP {
                c.dragged = true;
            }
            let c = *c;
            if c.role == 1 {
                self.send(play::POINTER, x, y);
            }
            if c.role == 2 {
                let values = [y - c.y < -22, x - c.x < -22, y - c.y > 22, x - c.x > 22];
                for (i, key) in "wasd".chars().enumerate() {
                    if values[i] != self.movement[i] {
                        self.movement[i] = values[i];
                        self.send(play::KEY, key as i32, values[i] as i32);
                    }
                }
            }
            if c.role == 13 {
                self.send(play::LOOK, (x - c.x).clamp(-160, 160), (y - c.y).clamp(-160, 160));
                self.contact.x = x;
                self.contact.y = y;
            }
        }
        Touch::Desktop
    }

    /// A tap: down and up at the same point and instant.
    pub fn tap(&mut self, px: i32, py: i32, now_ms: u64) -> Touch {
        match self.pointer(true, px, py, now_ms) {
            Touch::Keyboard => Touch::Keyboard,
            Touch::Desktop => self.pointer(false, px, py, now_ms),
        }
    }

    pub fn show_keyboard(&mut self, mode: KeyboardMode) {
        if self.keyboard.is_none() {
            self.keyboard_changed = true;
        }
        self.keyboard = Some(mode);
    }

    pub fn hide_keyboard(&mut self) {
        if self.keyboard.take().is_some() {
            self.keyboard_changed = true;
        }
    }

    fn press(&mut self, key: u32) {
        self.send(play::KEY, key as i32, 1);
        self.send(play::KEY, key as i32, 0);
    }

    /// One key of AERA's on-screen keyboard, as its `LV_EVENT_VALUE_CHANGED`
    /// handler forwards it. `unicode` is the negotiated desktop-integration
    /// extension; without it only printable ASCII, backspace and the action
    /// key exist. `Err` says why the keyboard has no such key.
    pub fn type_char(&mut self, c: char, unicode: bool) -> Result<(), String> {
        let Some(mode) = self.keyboard else {
            return Err("AERA's keyboard is hidden".into());
        };
        match c {
            '\u{8}' => self.press(BACKSPACE),
            '\n' if mode == KeyboardMode::Multiline => self.press(RETURN),
            // The extension submits single-line input before hiding; base
            // upstream's READY event only hides the keyboard.
            '\n' => {
                if unicode {
                    self.press(RETURN);
                }
                self.hide_keyboard();
            }
            c if mode == KeyboardMode::Number && !(c.is_ascii_digit() || c == '.') => {
                return Err(format!("{c:?} is not on the number keyboard"));
            }
            ' '..='~' => {
                let key = c as u32;
                let shift = c.is_ascii_uppercase() || "~!@#$%^&*()_+{}|:\"<>?".contains(c);
                if shift {
                    self.send(play::KEY, SHIFT_L as i32, 1);
                }
                self.press(key);
                if shift {
                    self.send(play::KEY, SHIFT_L as i32, 0);
                }
            }
            c if !unicode => return Err(format!("{c:?} is not ASCII; the base API 2 keyboard drops it")),
            c if c.is_control() => return Err(format!("{c:?} is a control character")),
            c => self.press(keysym(c)),
        }
        Ok(())
    }
}

/// The X11 keysym for a non-ASCII character: Latin-1 directly, anything
/// else `0x01000000 | scalar`.
pub fn keysym(c: char) -> u32 {
    match c as u32 {
        v @ 0xa0..=0xff => v,
        v => UNICODE | v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(d: &mut Desktop) -> Vec<(u32, i32, i32)> {
        d.out.drain(..).map(|p| (p.kind, p.x, p.y)).collect()
    }

    #[test]
    fn frames_start_at_one_alternate_slots_and_ack_before_the_next_read() {
        let mut d = Desktop::new(960, 540);
        assert_eq!(d.latched_slot(), None);
        assert_eq!(d.tick(), Tick::Receive);
        assert!(d.out.is_empty());
        assert_eq!(d.frame(Play { kind: play::FRAME, sequence: 1, x: 960, y: 540 }), Ok(1));
        // The next tick acknowledges frame 1, then reads.
        assert_eq!(d.tick(), Tick::Receive);
        assert_eq!(d.out.drain(..).collect::<Vec<_>>(), vec![Play { kind: play::ACK, sequence: 1, x: 0, y: 0 }]);
        assert_eq!(d.frame(Play { kind: play::FRAME, sequence: 2, x: 960, y: 540 }), Ok(0));
        // No new frame: one ACK only.
        d.tick();
        d.tick();
        assert_eq!(kinds(&mut d), vec![(play::ACK, 0, 0)]);
    }

    #[test]
    fn a_skipped_sequence_wrong_size_or_kind_closes_the_channel() {
        let mut d = Desktop::new(1280, 720);
        assert!(d.frame(Play { kind: play::FRAME, sequence: 2, x: 1280, y: 720 }).is_err());
        assert_eq!(d.tick(), Tick::Idle);
        let mut d = Desktop::new(1280, 720);
        assert!(d.frame(Play { kind: play::FRAME, sequence: 1, x: 960, y: 540 }).is_err());
        let mut d = Desktop::new(1280, 720);
        assert!(d.frame(Play { kind: play::KEY, sequence: 1, x: 1280, y: 720 }).is_err());
        assert!(!d.connected());
        d.release();
        assert!(d.out.is_empty());
    }

    #[test]
    fn menu_taps_point_and_click_scaled_to_the_frame() {
        let mut d = Desktop::new(1920, 1080);
        assert_eq!(d.tap(1000, 800, 0), Touch::Desktop);
        assert_eq!(
            kinds(&mut d),
            vec![(play::POINTER, 1000, 800), (play::BUTTON, 1, 1), (play::BUTTON, 1, 0)]
        );
    }

    #[test]
    fn the_toolbar_masks_the_top_left_strip() {
        let mut d = Desktop::new(960, 540);
        d.tap(240, 32, 0);
        assert_eq!(kinds(&mut d), vec![(play::KEY, ESCAPE as i32, 1), (play::KEY, ESCAPE as i32, 0)]);
        d.tap(445, 32, 0);
        assert_eq!(kinds(&mut d), vec![(play::KEY, RETURN as i32, 1), (play::KEY, RETURN as i32, 0)]);
        d.tap(535, 32, 0);
        assert_eq!(kinds(&mut d), vec![(play::KEY, TAB as i32, 1), (play::KEY, TAB as i32, 0)]);
        // Right of the toolbar and below it the app gets the pointer.
        d.tap(700, 32, 0);
        assert_eq!(kinds(&mut d)[0], (play::POINTER, 700, 32));
        d.tap(40, 70, 0);
        assert_eq!(kinds(&mut d)[0], (play::POINTER, 40, 70));
        // EXIT closes on the next tick.
        d.tap(40, 32, 0);
        assert!(d.out.is_empty());
        assert_eq!(d.tick(), Tick::Close);
    }

    #[test]
    fn play_mode_turns_touches_into_game_controls() {
        let mut d = Desktop::new(960, 540);
        d.tap(130, 32, 0);
        assert!(!d.menu());
        assert_eq!(kinds(&mut d), vec![(play::RELEASE, 0, 0)]);
        d.tap(905, 485, 0);
        assert_eq!(kinds(&mut d), vec![(play::BUTTON, 1, 1), (play::BUTTON, 1, 0)]);
        // A free-area tap looks by zero, then clicks.
        d.tap(600, 200, 0);
        assert_eq!(kinds(&mut d), vec![(play::LOOK, 0, 0), (play::BUTTON, 1, 1), (play::BUTTON, 1, 0)]);
        // Too slow for a tap.
        d.pointer(true, 600, 200, 0);
        d.pointer(false, 600, 200, 400);
        assert_eq!(kinds(&mut d), vec![(play::LOOK, 0, 0)]);
    }

    #[test]
    fn keys_shows_the_keyboard_which_takes_touches_and_types_ascii() {
        let mut d = Desktop::new(960, 540);
        assert!(d.type_char('a', false).is_err());
        d.tap(345, 32, 0);
        assert_eq!(kinds(&mut d), vec![(play::RELEASE, 0, 0)]);
        assert_eq!(d.take_keyboard_change(), Some(true));
        assert_eq!(d.keyboard_inset(), 297);
        assert_eq!(d.tap(500, 100, 0), Touch::Keyboard);
        d.type_char('H', false).unwrap();
        d.type_char('i', false).unwrap();
        assert_eq!(
            kinds(&mut d),
            vec![
                (play::KEY, SHIFT_L as i32, 1),
                (play::KEY, 'H' as i32, 1),
                (play::KEY, 'H' as i32, 0),
                (play::KEY, SHIFT_L as i32, 0),
                (play::KEY, 'i' as i32, 1),
                (play::KEY, 'i' as i32, 0),
            ]
        );
        assert!(d.type_char('é', false).is_err());
        assert!(d.out.is_empty());
        // OK hides it; no key goes out.
        d.type_char('\n', false).unwrap();
        assert!(d.out.is_empty());
        assert_eq!(d.take_keyboard_change(), Some(false));
        assert_eq!(d.take_keyboard_change(), None);
    }

    #[test]
    fn unicode_keys_are_x11_keysyms() {
        let mut d = Desktop::new(960, 540);
        d.show_keyboard(KeyboardMode::Multiline);
        d.type_char('é', true).unwrap();
        d.type_char('😀', true).unwrap();
        d.type_char('\n', true).unwrap();
        assert_eq!(
            kinds(&mut d),
            vec![
                (play::KEY, 0xe9, 1),
                (play::KEY, 0xe9, 0),
                (play::KEY, 0x0101_f600, 1),
                (play::KEY, 0x0101_f600, 0),
                (play::KEY, RETURN as i32, 1),
                (play::KEY, RETURN as i32, 0),
            ]
        );
        assert!(d.keyboard().is_some());
        d.show_keyboard(KeyboardMode::Number);
        assert!(d.type_char('a', true).is_err());
        d.type_char('7', true).unwrap();
        d.out.clear();
        d.type_char('\n', true).unwrap();
        assert_eq!(kinds(&mut d), vec![(play::KEY, RETURN as i32, 1), (play::KEY, RETURN as i32, 0)]);
        assert!(d.keyboard().is_none());
    }
}
