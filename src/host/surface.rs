//! The pixel surface: geometry from `SURFACE` and the shared frame slots.
//!
//! Host API 3: a sealed memfd holding `slots` BGRA8888 top-down frames of
//! `stride * height` bytes. The plugin writes a free slot, sends `PRESENT`,
//! and may not touch that slot again until the host answers `FRAME_DONE`
//! for it (AERA keeps the frame on screen until a newer one replaces it).
//!
//! A later `SURFACE` (AERA rotated) reshapes the slots within the same
//! memfd and starts a new generation: every slot is ours again, and
//! `PRESENT` carries the generation its frame was drawn for in `flags`, so
//! AERA hands back a frame of the old shape unshown.

use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use super::{kind, Message};

pub const FORMAT: &str = "BGRA8888";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub width: u32,
    pub height: u32,
    /// Bytes per row, at least `width * 4`.
    pub stride: u32,
    pub slots: u32,
    /// Physical pixels per logical pixel.
    pub scale: f64,
    /// The panel's refresh rate, for frame timing.
    pub refresh_hz: f64,
}

impl Geometry {
    /// A phone panel like the ones AERA runs on, used by the simulator.
    pub const PHONE: Geometry =
        Geometry { width: 1080, height: 2400, stride: 1080 * 4, slots: 3, scale: 2.75, refresh_hz: 60.0 };

    pub fn frame_bytes(&self) -> usize {
        self.stride as usize * self.height as usize
    }

    pub fn total_bytes(&self) -> usize {
        self.frame_bytes() * self.slots as usize
    }

    pub fn valid(&self) -> bool {
        (1..=8192).contains(&self.width)
            && (1..=8192).contains(&self.height)
            && self.stride >= self.width * 4
            && self.stride % 4 == 0
            && (1..=4).contains(&self.slots)
            && (0.5..=8.0).contains(&self.scale)
            && (1.0..=240.0).contains(&self.refresh_hz)
    }

    pub fn to_message(&self) -> Message {
        Message {
            title: FORMAT.into(),
            text: format!("slots={} scale={} refresh={}", self.slots, self.scale, self.refresh_hz),
            ..Message::with(kind::SURFACE, self.stride, self.width, self.height)
        }
    }

    pub fn from_message(message: &Message) -> Option<Geometry> {
        if message.kind != kind::SURFACE || message.title != FORMAT {
            return None;
        }
        let mut g = Geometry {
            width: message.value,
            height: message.flags,
            stride: message.request_id,
            slots: 3,
            scale: 1.0,
            refresh_hz: 60.0,
        };
        for pair in message.text.split_whitespace() {
            match pair.split_once('=') {
                Some(("slots", v)) => g.slots = v.parse().ok()?,
                Some(("scale", v)) => g.scale = v.parse().ok()?,
                Some(("refresh", v)) => g.refresh_hz = v.parse().ok()?,
                // Unknown keys are for later hosts.
                _ => {}
            }
        }
        g.valid().then_some(g)
    }

    pub fn frame_period(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.refresh_hz)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlotState {
    Free,
    /// Handed to the renderer, not presented yet.
    Writing,
    /// Presented as this sequence; the host owns it until `FRAME_DONE`.
    Presented(u32),
}

struct State {
    geometry: Geometry,
    generation: u32,
    slots: Vec<SlotState>,
    next_sequence: u32,
    closed: bool,
}

/// A slot the renderer holds, with the shape it was taken for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub index: usize,
    pub geometry: Geometry,
    generation: u32,
}

/// The mapped memfd and who owns each slot.
pub struct Slots {
    base: *mut u8,
    len: usize,
    _fd: OwnedFd,
    state: Mutex<State>,
    freed: Condvar,
}

// SAFETY: the mapping lives as long as `Slots`; each slot's bytes are only
// written by whoever holds it in `Writing`, which `state` hands out to one
// caller at a time.
unsafe impl Send for Slots {}
unsafe impl Sync for Slots {}

impl Slots {
    /// Maps the host's frame memory.
    pub fn map(fd: OwnedFd, geometry: Geometry) -> io::Result<Slots> {
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        // SAFETY: fd is open for as long as `fd` lives; stat is a valid out
        // pointer.
        if unsafe { libc::fstat(fd.as_raw_fd(), &mut stat) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let len = stat.st_size as usize;
        if len < geometry.total_bytes() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "surface memfd is smaller than SURFACE says"));
        }
        // All of it: AERA sizes it for every shape a later SURFACE may give.
        // SAFETY: a fresh shared mapping of a file this large.
        let base = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };
        if base == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Slots {
            base: base.cast(),
            len,
            _fd: fd,
            state: Mutex::new(State {
                geometry,
                generation: 0,
                slots: vec![SlotState::Free; geometry.slots as usize],
                next_sequence: 1,
                closed: false,
            }),
            freed: Condvar::new(),
        })
    }

    pub fn geometry(&self) -> Geometry {
        self.state.lock().unwrap().geometry
    }

    /// The longest side a frame may take in any later `SURFACE`: AERA sizes
    /// the memfd as `slots` square frames on its screen's longer side, so a
    /// rotation can widen a frame beyond the first shape's longer side.
    pub fn longest_side(&self) -> u32 {
        let g = self.geometry();
        let per_slot = self.len / g.slots.max(1) as usize / 4;
        let mut side = (per_slot as f64).sqrt() as usize;
        while (side + 1) * (side + 1) <= per_slot {
            side += 1;
        }
        while side * side > per_slot {
            side -= 1;
        }
        (side as u32).max(g.width.max(g.height))
    }

    /// A later `SURFACE`: the new shape within the same memory, a new
    /// generation, and every slot ours again (a frame still being written
    /// is presented as the old generation, which AERA hands straight back).
    /// False when the shape does not fit the memfd or changes the slot count.
    pub fn reshape(&self, geometry: Geometry) -> bool {
        let mut state = self.state.lock().unwrap();
        if geometry.total_bytes() > self.len || geometry.slots != state.geometry.slots {
            return false;
        }
        state.geometry = geometry;
        state.generation = state.generation.wrapping_add(1);
        for slot in state.slots.iter_mut() {
            if matches!(slot, SlotState::Presented(_)) {
                *slot = SlotState::Free;
            }
        }
        self.freed.notify_all();
        true
    }

    /// Whether a slot is free right now (vsync is only granted then).
    pub fn has_free(&self) -> bool {
        self.state.lock().unwrap().slots.contains(&SlotState::Free)
    }

    /// Waits up to `timeout` for a free slot and takes it.
    pub fn acquire(&self, timeout: Duration) -> Option<Frame> {
        let state = self.state.lock().unwrap();
        let (mut state, _) = self
            .freed
            .wait_timeout_while(state, timeout, |s| !s.closed && !s.slots.contains(&SlotState::Free))
            .unwrap();
        if state.closed {
            return None;
        }
        let index = state.slots.iter().position(|s| *s == SlotState::Free)?;
        state.slots[index] = SlotState::Writing;
        Some(Frame { index, geometry: state.geometry, generation: state.generation })
    }

    /// The bytes of a slot the caller holds, laid out as when it was taken.
    ///
    /// # Safety
    /// `frame` must have come from [`Slots::acquire`] and not been presented
    /// or released since. Only the one renderer thread writes slots, so no
    /// one else touches these bytes (after a reshape, slots of the two
    /// shapes may overlap, but the renderer writes one frame at a time and
    /// AERA reads only frames of the current shape).
    #[allow(clippy::mut_from_ref)] // the slot is shared memory, owned per the contract above
    pub unsafe fn slot_mut(&self, frame: &Frame) -> &mut [u8] {
        let size = frame.geometry.frame_bytes();
        debug_assert!((frame.index + 1) * size <= self.len);
        std::slice::from_raw_parts_mut(self.base.add(frame.index * size), size)
    }

    /// Marks a written slot as presented and returns the `PRESENT` message,
    /// carrying the generation the frame was drawn for.
    pub fn present(&self, frame: &Frame) -> Message {
        let mut state = self.state.lock().unwrap();
        debug_assert_eq!(state.slots[frame.index], SlotState::Writing);
        let sequence = state.next_sequence;
        state.next_sequence = state.next_sequence.wrapping_add(1).max(1);
        state.slots[frame.index] = SlotState::Presented(sequence);
        Message::with(kind::PRESENT, sequence, frame.index as u32, frame.generation)
    }

    /// Gives back a slot that was acquired but not presented.
    pub fn release(&self, frame: &Frame) {
        let mut state = self.state.lock().unwrap();
        state.slots[frame.index] = SlotState::Free;
        self.freed.notify_all();
    }

    /// `FRAME_DONE` for `sequence`. Returns false for an unknown sequence.
    pub fn frame_done(&self, sequence: u32) -> bool {
        let mut state = self.state.lock().unwrap();
        let Some(i) = state.slots.iter().position(|s| *s == SlotState::Presented(sequence)) else {
            return false;
        };
        state.slots[i] = SlotState::Free;
        self.freed.notify_all();
        true
    }

    /// Wakes any waiter for good; used on shutdown.
    pub fn close(&self) {
        self.state.lock().unwrap().closed = true;
        self.freed.notify_all();
    }
}

impl Drop for Slots {
    fn drop(&mut self) {
        // SAFETY: the mapping made in `map`, unmapped once.
        unsafe { libc::munmap(self.base.cast(), self.len) };
    }
}

/// Creates a sealed memfd big enough for `geometry`, as the host does.
pub fn create_memfd(geometry: &Geometry) -> io::Result<OwnedFd> {
    create_memfd_of(geometry.total_bytes())
}

/// Creates a sealed memfd of `len` bytes.
pub fn create_memfd_of(len: usize) -> io::Result<OwnedFd> {
    use std::os::fd::FromRawFd;
    // SAFETY: a NUL-terminated name and valid flags.
    let fd = unsafe { libc::memfd_create(c"aera-surface".as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd was just created and is owned by nothing else.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    // SAFETY: fd is a valid memfd.
    unsafe {
        if libc::ftruncate(fd.as_raw_fd(), len as i64) != 0
            || libc::fcntl(fd.as_raw_fd(), libc::F_ADD_SEALS, libc::F_SEAL_SHRINK | libc::F_SEAL_GROW | libc::F_SEAL_SEAL) != 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(fd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_round_trip() {
        let g = Geometry::PHONE;
        assert_eq!(Geometry::from_message(&g.to_message()), Some(g));
    }

    #[test]
    fn geometry_rejects_bad_stride_and_format() {
        let mut m = Geometry::PHONE.to_message();
        m.request_id = 100;
        assert!(Geometry::from_message(&m).is_none());
        let mut m = Geometry::PHONE.to_message();
        m.title = "RGB565".into();
        assert!(Geometry::from_message(&m).is_none());
    }

    #[test]
    fn slots_flow_control() {
        let g = Geometry { width: 4, height: 4, stride: 16, slots: 3, scale: 1.0, refresh_hz: 60.0 };
        let slots = Slots::map(create_memfd(&g).unwrap(), g).unwrap();
        let short = Duration::from_millis(1);
        let mut sent = vec![];
        for _ in 0..3 {
            let f = slots.acquire(short).unwrap();
            unsafe { slots.slot_mut(&f).fill(f.index as u8) };
            sent.push(slots.present(&f));
        }
        assert!(!slots.has_free());
        assert!(slots.acquire(short).is_none());
        assert!(!slots.frame_done(999));
        assert!(slots.frame_done(sent[1].request_id));
        assert_eq!(slots.acquire(short).map(|f| f.index), Some(sent[1].value as usize));
        let sequences: Vec<u32> = sent.iter().map(|m| m.request_id).collect();
        assert_eq!(sequences, vec![1, 2, 3]);
    }

    #[test]
    fn longest_side_is_what_the_memfd_holds() {
        // A portrait view below a status bar: 720x1183 on a 720x1348 screen,
        // memfd sized for 1348x1348 frames. Landscape is 1348 wide.
        let g = Geometry { width: 720, height: 1183, stride: 2880, slots: 3, scale: 1.75, refresh_hz: 60.0 };
        let slots = Slots::map(create_memfd_of(1348 * 1348 * 4 * 3).unwrap(), g).unwrap();
        assert_eq!(slots.longest_side(), 1348);
        // A memfd of exactly the first shape still gives its longer side.
        let slots = Slots::map(create_memfd(&g).unwrap(), g).unwrap();
        assert_eq!(slots.longest_side(), 1183);
    }

    #[test]
    fn reshape_starts_a_generation() {
        let g = Geometry { width: 4, height: 2, stride: 16, slots: 3, scale: 1.0, refresh_hz: 60.0 };
        // Room for the rotated shape too, as AERA allocates it.
        let slots = Slots::map(create_memfd_of(4 * 4 * 4 * 3).unwrap(), g).unwrap();
        let short = Duration::from_millis(1);
        let shown = slots.acquire(short).unwrap();
        let first = slots.present(&shown);
        assert_eq!(first.flags, 0);
        let writing = slots.acquire(short).unwrap();
        let rotated = Geometry { width: 2, height: 4, stride: 8, ..g };
        assert!(slots.reshape(rotated));
        assert_eq!(slots.geometry(), rotated);
        // The presented slot is ours again; the one being written stays
        // taken and goes out as the old generation.
        assert!(!slots.frame_done(first.request_id));
        assert_eq!(slots.present(&writing).flags, 0);
        let next = slots.acquire(short).unwrap();
        assert_eq!(next.geometry, rotated);
        assert_eq!(slots.present(&next).flags, 1);
        // Too big for the memfd, or another slot count.
        assert!(!slots.reshape(Geometry { width: 64, stride: 256, ..g }));
        assert!(!slots.reshape(Geometry { slots: 2, ..rotated }));
    }
}
