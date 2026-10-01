//! Damage-only copies. Our framebuffer keeps the last frame, so Flutter
//! repaints only what changed (partial repaint) and reports it. Each AERA
//! slot still holds the frame we last wrote into it, so a slot needs the
//! union of what changed since then, not the whole frame.

use std::collections::VecDeque;

/// A pixel rectangle, `x0..x1` × `y0..y1`, top-left origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl Rect {
    pub fn full(width: u32, height: u32) -> Rect {
        Rect { x0: 0, y0: 0, x1: width, y1: height }
    }

    /// From Flutter's float rectangle, grown to whole pixels and clipped to
    /// the surface.
    pub fn from_float(l: f64, t: f64, r: f64, b: f64, width: u32, height: u32) -> Rect {
        let clamp = |v: f64, max: u32| v.max(0.0).min(max as f64);
        Rect {
            x0: clamp(l.floor(), width) as u32,
            y0: clamp(t.floor(), height) as u32,
            x1: clamp(r.ceil(), width) as u32,
            y1: clamp(b.ceil(), height) as u32,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }

    pub fn union(self, o: Rect) -> Rect {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return self;
        }
        Rect { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }
}

const HISTORY: usize = 8;

pub struct Damage {
    width: u32,
    height: u32,
    frame: u64,
    /// What changed in each recent frame, newest last; `None` = everything.
    history: VecDeque<Option<Rect>>,
    /// The frame each slot last received.
    slots: Vec<Option<u64>>,
}

impl Damage {
    pub fn new(width: u32, height: u32, slots: usize) -> Damage {
        Damage { width, height, frame: 0, history: VecDeque::new(), slots: vec![None; slots] }
    }

    /// A frame that went into no slot (dropped): later copies still need
    /// what it changed.
    pub fn skip(&mut self, changed: Option<Rect>) {
        self.frame += 1;
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(changed);
    }

    /// A new frame changed `changed` (`None`: unknown, everything) and goes
    /// into `slot`. Returns the part of the slot to copy.
    pub fn next(&mut self, slot: usize, changed: Option<Rect>) -> Rect {
        self.skip(changed);
        let full = Rect::full(self.width, self.height);
        let age = self.slots[slot].map(|f| self.frame - f);
        self.slots[slot] = Some(self.frame);
        match age {
            Some(age) if age as usize <= self.history.len() => self
                .history
                .iter()
                .rev()
                .take(age as usize)
                .try_fold(Rect { x0: 0, y0: 0, x1: 0, y1: 0 }, |acc, d| d.map(|d| acc.union(d)))
                .unwrap_or(full),
            _ => full,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x0: u32, y0: u32, x1: u32, y1: u32) -> Rect {
        Rect { x0, y0, x1, y1 }
    }

    #[test]
    fn slots_collect_what_they_missed() {
        let mut d = Damage::new(100, 200, 3);
        let full = r(0, 0, 100, 200);
        // Each slot's first frame is copied whole.
        assert_eq!(d.next(0, Some(r(1, 1, 2, 2))), full);
        assert_eq!(d.next(1, Some(r(10, 10, 20, 20))), full);
        assert_eq!(d.next(2, Some(r(30, 30, 40, 40))), full);
        // Slot 0 missed frames 2 and 3, and frame 4 changes more.
        assert_eq!(d.next(0, Some(r(50, 50, 60, 60))), r(10, 10, 60, 60));
        // Slot 1 missed 3 and 4; frame 5 changed nothing.
        assert_eq!(d.next(1, Some(r(0, 0, 0, 0))), r(30, 30, 60, 60));
        // Slot 1 again straight away: only frame 6.
        assert_eq!(d.next(1, Some(r(5, 5, 6, 6))), r(5, 5, 6, 6));
        // An unknown frame forces whole copies until every slot has it.
        assert_eq!(d.next(2, None), full);
        assert_eq!(d.next(0, Some(r(5, 5, 6, 6))), full);
    }

    #[test]
    fn dropped_frames_still_count() {
        let mut d = Damage::new(100, 100, 1);
        d.next(0, None);
        d.skip(Some(r(1, 1, 2, 2)));
        assert_eq!(d.next(0, Some(r(5, 5, 6, 6))), r(1, 1, 6, 6));
    }

    #[test]
    fn old_slots_are_copied_whole() {
        let mut d = Damage::new(10, 10, 2);
        d.next(0, None);
        for _ in 0..HISTORY + 1 {
            d.next(1, Some(r(1, 1, 2, 2)));
        }
        assert_eq!(d.next(0, Some(r(1, 1, 2, 2))), Rect::full(10, 10));
    }

    #[test]
    fn float_rects_grow_and_clip() {
        assert_eq!(Rect::from_float(-3.0, 1.5, 9.2, 400.0, 10, 20), r(0, 1, 10, 20));
    }
}
