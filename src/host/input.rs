//! Desktop input from the play channel as what Flutter understands, without
//! the engine: `engine.rs` turns each [`Input`] into a pointer event or a
//! handler call.
//!
//! What AERA sends is one pointer, not raw touches: in its MENU mode a touch
//! is `POINTER` then `BUTTON 1`, a mouse the same with buttons 1–3 and
//! `WHEEL`. Button 1 becomes a single touch pointer (so Flutter's scroll
//! views drag, as on a phone); buttons 2 and 3 a mouse's middle and
//! secondary buttons, the wheel a mouse scroll. `LOOK` is the PLAY mode's
//! relative game motion, which Flutter has no use for. Keys are X11
//! keysyms: Escape is Back, text keys go to the text input model.

use super::play::{self, PlayMessage};

/// Flutter `kFlutterPointerButtonMouse*` bits.
pub const PRIMARY: i64 = 1;
pub const SECONDARY: i64 = 2;
pub const MIDDLE: i64 = 4;

/// The pointer devices we report.
pub const TOUCH_DEVICE: i32 = 0;
pub const MOUSE_DEVICE: i32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    Touch,
    Mouse,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Add,
    Down,
    Move,
    Up,
    Cancel,
    /// A mouse not pressed, carrying a scroll signal.
    Hover,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    Pointer {
        device: Device,
        phase: Phase,
        x: f64,
        y: f64,
        /// Mouse buttons held after this event.
        buttons: i64,
        /// Wheel scroll in pixels, downward positive (Flutter's sign).
        scroll_y: f64,
    },
    /// Flutter's Back (`popRoute`).
    Back,
    /// A text key for the input model: a code point, 8 backspace, 13 enter.
    Text(u32),
}

/// Pixels per wheel step, as GTK's embedder scrolls.
pub const SCROLL_STEP: f64 = 53.0;

/// X11 keysyms the desktop sends that we use.
pub mod keysym {
    pub const BACKSPACE: u32 = 0xff08;
    pub const TAB: u32 = 0xff09;
    pub const RETURN: u32 = 0xff0d;
    pub const ESCAPE: u32 = 0xff1b;
    pub const KP_ENTER: u32 = 0xff8d;
    /// X11's Unicode keysym range: `0x01000000 | scalar`.
    pub const UNICODE: u32 = 0x0100_0000;
}

/// A keysym as text for the input model. Latin-1 printable keysyms are their
/// code point; Unicode keysyms only count when `unicode` (the desktop
/// integration extension) was negotiated, as base API 2 sends none.
pub fn keysym_text(sym: u32, unicode: bool) -> Option<u32> {
    match sym {
        keysym::BACKSPACE => Some(8),
        keysym::RETURN | keysym::KP_ENTER => Some(13),
        0x20..=0x7e | 0xa0..=0xff => Some(sym),
        _ if unicode && sym & 0xff00_0000 == keysym::UNICODE => {
            let scalar = sym & 0x00ff_ffff;
            (scalar >= 0x100 && char::from_u32(scalar).is_some()).then_some(scalar)
        }
        _ => None,
    }
}

fn mouse_bit(button: i32) -> Option<i64> {
    match button {
        2 => Some(MIDDLE),
        3 => Some(SECONDARY),
        _ => None,
    }
}

/// The input state of one desktop session.
#[derive(Clone, Debug, Default)]
pub struct Translator {
    x: f64,
    y: f64,
    touching: bool,
    mouse_added: bool,
    buttons: i64,
    /// Keys down, so `RELEASE` and key-up stay consistent.
    keys: Vec<u32>,
    width: f64,
    height: f64,
}

impl Translator {
    pub fn new(width: u32, height: u32) -> Translator {
        Translator { width: width as f64, height: height as f64, ..Default::default() }
    }

    fn pointer(&self, device: Device, phase: Phase) -> Input {
        Input::Pointer { device, phase, x: self.x, y: self.y, buttons: self.buttons, scroll_y: 0.0 }
    }

    fn mouse(&mut self, out: &mut Vec<Input>) {
        if !self.mouse_added {
            self.mouse_added = true;
            out.push(Input::Pointer { device: Device::Mouse, phase: Phase::Add, x: self.x, y: self.y, buttons: 0, scroll_y: 0.0 });
        }
    }

    /// One host packet. `unicode` as in [`keysym_text`]. `FRAME`, `ACK` and
    /// `CLOSE` are the caller's and give nothing here.
    pub fn on_play(&mut self, m: &PlayMessage, unicode: bool) -> Vec<Input> {
        let mut out = vec![];
        match m.kind {
            play::POINTER => {
                self.x = (m.x as f64).clamp(0.0, (self.width - 1.0).max(0.0));
                self.y = (m.y as f64).clamp(0.0, (self.height - 1.0).max(0.0));
                if self.touching {
                    out.push(self.pointer(Device::Touch, Phase::Move));
                }
                if self.buttons != 0 {
                    out.push(self.pointer(Device::Mouse, Phase::Move));
                }
            }
            play::BUTTON => {
                let down = m.y != 0;
                if m.x == 1 {
                    if down != self.touching {
                        self.touching = down;
                        out.push(self.pointer(Device::Touch, if down { Phase::Down } else { Phase::Up }));
                    }
                } else if let Some(bit) = mouse_bit(m.x) {
                    let held = self.buttons & bit != 0;
                    if down != held {
                        self.mouse(&mut out);
                        let before = self.buttons;
                        self.buttons ^= bit;
                        let phase = match (before, self.buttons) {
                            (0, _) => Phase::Down,
                            (_, 0) => Phase::Up,
                            _ => Phase::Move,
                        };
                        out.push(self.pointer(Device::Mouse, phase));
                    }
                }
            }
            play::WHEEL if m.x != 0 => {
                self.mouse(&mut out);
                let phase = if self.buttons != 0 { Phase::Move } else { Phase::Hover };
                // Wheel up (positive) scrolls content back: negative delta.
                let scroll_y = -(m.x.clamp(-8, 8) as f64) * SCROLL_STEP;
                out.push(Input::Pointer { device: Device::Mouse, phase, x: self.x, y: self.y, buttons: self.buttons, scroll_y });
            }
            play::KEY => {
                let sym = m.x as u32;
                let held = self.keys.contains(&sym);
                if m.y != 0 && !held {
                    self.keys.push(sym);
                    if sym == keysym::ESCAPE {
                        out.push(Input::Back);
                    } else if let Some(code) = keysym_text(sym, unicode) {
                        out.push(Input::Text(code));
                    }
                } else if m.y == 0 && held {
                    self.keys.retain(|&k| k != sym);
                }
            }
            play::RELEASE => out = self.release(),
            _ => {}
        }
        out
    }

    /// Everything held is let go: the gesture is cancelled, not completed.
    pub fn release(&mut self) -> Vec<Input> {
        let mut out = vec![];
        if self.touching {
            self.touching = false;
            out.push(self.pointer(Device::Touch, Phase::Cancel));
        }
        if self.buttons != 0 {
            self.buttons = 0;
            out.push(self.pointer(Device::Mouse, Phase::Cancel));
        }
        self.keys.clear();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(kind: u32, x: i32, y: i32) -> PlayMessage {
        PlayMessage::new(kind, 0, x, y)
    }

    fn phases(v: &[Input]) -> Vec<(Device, Phase)> {
        v.iter()
            .filter_map(|i| match i {
                Input::Pointer { device, phase, .. } => Some((*device, *phase)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_tap_is_a_touch() {
        let mut t = Translator::new(960, 540);
        assert!(t.on_play(&p(play::POINTER, 100, 200), false).is_empty(), "no hover for a touch");
        let down = t.on_play(&p(play::BUTTON, 1, 1), false);
        assert_eq!(down, vec![Input::Pointer { device: Device::Touch, phase: Phase::Down, x: 100.0, y: 200.0, buttons: 0, scroll_y: 0.0 }]);
        assert_eq!(phases(&t.on_play(&p(play::POINTER, 2000, -5), false)), vec![(Device::Touch, Phase::Move)]);
        let up = t.on_play(&p(play::BUTTON, 1, 0), false);
        assert!(matches!(up[0], Input::Pointer { phase: Phase::Up, x, y, .. } if x == 959.0 && y == 0.0));
        assert!(t.on_play(&p(play::BUTTON, 1, 0), false).is_empty(), "no double up");
    }

    #[test]
    fn mouse_buttons_and_wheel() {
        let mut t = Translator::new(960, 540);
        let v = t.on_play(&p(play::BUTTON, 3, 1), false);
        assert_eq!(phases(&v), vec![(Device::Mouse, Phase::Add), (Device::Mouse, Phase::Down)]);
        assert!(matches!(v[1], Input::Pointer { buttons: SECONDARY, .. }));
        let v = t.on_play(&p(play::BUTTON, 2, 1), false);
        assert!(matches!(v[0], Input::Pointer { phase: Phase::Move, buttons, .. } if buttons == SECONDARY | MIDDLE));
        t.on_play(&p(play::BUTTON, 3, 0), false);
        assert_eq!(phases(&t.on_play(&p(play::BUTTON, 2, 0), false)), vec![(Device::Mouse, Phase::Up)]);
        let v = t.on_play(&p(play::WHEEL, 2, 0), false);
        assert!(matches!(v[0], Input::Pointer { phase: Phase::Hover, scroll_y, .. } if scroll_y == -2.0 * SCROLL_STEP));
        assert!(t.on_play(&p(play::WHEEL, 0, 0), false).is_empty());
        assert!(t.on_play(&p(play::BUTTON, 9, 1), false).is_empty());
        assert!(t.on_play(&p(play::LOOK, 10, 10), false).is_empty());
    }

    #[test]
    fn keys() {
        let mut t = Translator::new(960, 540);
        assert_eq!(t.on_play(&p(play::KEY, keysym::ESCAPE as i32, 1), false), vec![Input::Back]);
        assert!(t.on_play(&p(play::KEY, keysym::ESCAPE as i32, 1), false).is_empty(), "no edge without key-up");
        t.on_play(&p(play::KEY, keysym::ESCAPE as i32, 0), false);
        assert_eq!(t.on_play(&p(play::KEY, 'a' as i32, 1), false), vec![Input::Text('a' as u32)]);
        assert!(t.on_play(&p(play::KEY, 'a' as i32, 0), false).is_empty());
        assert_eq!(t.on_play(&p(play::KEY, keysym::RETURN as i32, 1), false), vec![Input::Text(13)]);
        assert!(t.on_play(&p(play::KEY, 0xffe1, 1), false).is_empty(), "shift is not text");
        let euro = (keysym::UNICODE | 0x20ac) as i32;
        assert!(t.on_play(&p(play::KEY, euro, 1), false).is_empty(), "base API 2 has no Unicode keysyms");
        t.on_play(&p(play::KEY, euro, 0), false);
        assert_eq!(t.on_play(&p(play::KEY, euro, 1), true), vec![Input::Text(0x20ac)]);
    }

    #[test]
    fn keysyms() {
        assert_eq!(keysym_text(0xe9, false), Some(0xe9));
        assert_eq!(keysym_text(keysym::BACKSPACE, false), Some(8));
        assert_eq!(keysym_text(keysym::TAB, true), None);
        assert_eq!(keysym_text(keysym::UNICODE | 0xd800, true), None, "surrogates are not scalars");
        assert_eq!(keysym_text(keysym::UNICODE | 0x41, true), None, "Latin-1 travels as itself");
    }

    #[test]
    fn release_cancels_everything_held() {
        let mut t = Translator::new(960, 540);
        t.on_play(&p(play::BUTTON, 1, 1), false);
        t.on_play(&p(play::BUTTON, 3, 1), false);
        t.on_play(&p(play::KEY, 'w' as i32, 1), false);
        let v = t.on_play(&p(play::RELEASE, 0, 0), false);
        assert_eq!(phases(&v), vec![(Device::Touch, Phase::Cancel), (Device::Mouse, Phase::Cancel)]);
        assert!(t.on_play(&p(play::RELEASE, 0, 0), false).is_empty());
        // The key is up again: pressing it is a new edge.
        assert_eq!(t.on_play(&p(play::KEY, 'w' as i32, 1), false), vec![Input::Text('w' as u32)]);
    }
}
