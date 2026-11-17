//! The desktop's frame memory: the host's sealed memfd of two top-down
//! BGRA8888 frames of `width * 4 * height` bytes, and the upstream
//! sequence rule for them.
//!
//! Frame `N` (from 1, exactly one more each time) goes into slot `N % 2`,
//! then `PLAY_FRAME` names it. AERA's 16 ms timer latches a frame and
//! acknowledges it on its next tick, and keeps showing a slot until a newer
//! frame is latched. So the slot of `N + 1` is only free once `N - 1` is
//! replaced, which is once `N` is acknowledged: one frame is in flight at a
//! time, and a frame is only written after the previous one's `PLAY_ACK`.
//! `ACK` is not vsync, and there is no damage, resize or timestamp.

use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use super::play::{self, PlayMessage};

pub const FORMAT: &str = "BGRA8888";
/// Upstream's double buffer.
pub const SLOTS: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub width: u32,
    pub height: u32,
    /// Bytes per row: `width * 4`, as upstream has no stride.
    pub stride: u32,
    pub slots: u32,
    /// Physical pixels per logical pixel: 1, the desktop is scaled by AERA.
    pub scale: f64,
    /// AERA's timer rate, for frame timing; not a measured panel rate.
    pub refresh_hz: f64,
}

impl Geometry {
    /// The desktop surface of `width` x `height`.
    pub const fn desktop(width: u32, height: u32) -> Geometry {
        Geometry { width, height, stride: width * 4, slots: SLOTS, scale: 1.0, refresh_hz: 60.0 }
    }

    pub fn frame_bytes(&self) -> usize {
        self.stride as usize * self.height as usize
    }

    pub fn total_bytes(&self) -> usize {
        self.frame_bytes() * self.slots as usize
    }

    pub fn frame_period(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.refresh_hz)
    }
}

/// Why a frame acknowledgement or channel packet broke the protocol.
#[derive(Debug, PartialEq, Eq)]
pub enum AckError {
    /// `ACK` with no frame in flight, or for another sequence.
    Unexpected { sequence: u32, in_flight: Option<u32> },
}

impl std::fmt::Display for AckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AckError::Unexpected { sequence, in_flight } => write!(f, "ACK {sequence} while {in_flight:?} is in flight"),
        }
    }
}

struct State {
    /// The next frame's sequence; slot `next % 2`.
    next: u32,
    /// Sent and not acknowledged yet.
    in_flight: Option<u32>,
    /// The renderer holds the next slot.
    writing: bool,
    closed: bool,
}

/// A slot the renderer holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub index: usize,
    pub sequence: u32,
    pub geometry: Geometry,
}

/// The mapped memfd and the sequence that owns its slots.
pub struct Slots {
    base: *mut u8,
    len: usize,
    geometry: Geometry,
    _fd: OwnedFd,
    state: Mutex<State>,
    freed: Condvar,
}

// SAFETY: the mapping lives as long as `Slots`; a slot's bytes are only
// written by whoever holds it as `writing`, which `state` hands out to one
// caller at a time.
unsafe impl Send for Slots {}
unsafe impl Sync for Slots {}

impl Slots {
    /// Maps the host's frame memory. The memfd must hold both frames and be
    /// sealed against shrinking, so the host cannot cut the mapping under
    /// us.
    pub fn map(fd: OwnedFd, geometry: Geometry) -> io::Result<Slots> {
        // SAFETY: a plain C struct; zeroed is valid.
        let mut stat: libc::stat = unsafe { std::mem::zeroed() };
        // SAFETY: fd is open; stat is a valid out pointer.
        if unsafe { libc::fstat(fd.as_raw_fd(), &mut stat) } != 0 {
            return Err(io::Error::last_os_error());
        }
        if stat.st_mode & libc::S_IFMT != libc::S_IFREG {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "desktop memory is not a memfd"));
        }
        let len = geometry.total_bytes();
        if (stat.st_size as u64) < len as u64 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "desktop memfd is smaller than two frames"));
        }
        // SAFETY: fd is open.
        let seals = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GET_SEALS) };
        if seals < 0 || seals & libc::F_SEAL_SHRINK == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "desktop memfd is not sealed against shrinking"));
        }
        // SAFETY: a fresh shared mapping within a file this large that
        // cannot shrink.
        let base = unsafe {
            libc::mmap(std::ptr::null_mut(), len, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd.as_raw_fd(), 0)
        };
        if base == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Slots {
            base: base.cast(),
            len,
            geometry,
            _fd: fd,
            state: Mutex::new(State { next: 1, in_flight: None, writing: false, closed: false }),
            freed: Condvar::new(),
        })
    }

    pub fn geometry(&self) -> Geometry {
        self.geometry
    }

    /// Whether the next slot may be written now (vsync is only granted then).
    pub fn has_free(&self) -> bool {
        let s = self.state.lock().unwrap();
        !s.closed && !s.writing && s.in_flight.is_none()
    }

    /// Waits up to `timeout` for the previous frame's `ACK` and takes the
    /// next slot.
    pub fn acquire(&self, timeout: Duration) -> Option<Frame> {
        let state = self.state.lock().unwrap();
        let (mut state, _) = self
            .freed
            .wait_timeout_while(state, timeout, |s| !s.closed && (s.writing || s.in_flight.is_some()))
            .unwrap();
        if state.closed || state.writing || state.in_flight.is_some() {
            return None;
        }
        state.writing = true;
        let sequence = state.next;
        Some(Frame { index: (sequence % SLOTS) as usize, sequence, geometry: self.geometry })
    }

    /// The bytes of the slot the caller holds.
    ///
    /// # Safety
    /// `frame` must have come from [`Slots::acquire`] on this `Slots` and not
    /// been presented or released since; only its holder touches these
    /// bytes, and AERA is not showing this slot (see the module docs).
    #[allow(clippy::mut_from_ref)] // the slot is shared memory, owned per the contract above
    pub unsafe fn slot_mut(&self, frame: &Frame) -> &mut [u8] {
        let size = self.geometry.frame_bytes();
        debug_assert!((frame.index + 1) * size <= self.len);
        std::slice::from_raw_parts_mut(self.base.add(frame.index * size), size)
    }

    /// Marks the written slot as in flight and returns its `PLAY_FRAME`.
    pub fn present(&self, frame: &Frame) -> PlayMessage {
        let mut state = self.state.lock().unwrap();
        debug_assert!(state.writing && state.next == frame.sequence);
        state.writing = false;
        state.in_flight = Some(frame.sequence);
        state.next = frame.sequence.wrapping_add(1);
        PlayMessage::new(play::FRAME, frame.sequence, self.geometry.width as i32, self.geometry.height as i32)
    }

    /// Gives back a slot that was acquired but not presented; its sequence
    /// is used by the next frame.
    pub fn release(&self, frame: &Frame) {
        let mut state = self.state.lock().unwrap();
        debug_assert_eq!(state.next, frame.sequence);
        state.writing = false;
        self.freed.notify_all();
    }

    /// `PLAY_ACK` for `sequence`: must be the frame in flight.
    pub fn ack(&self, sequence: u32) -> Result<(), AckError> {
        let mut state = self.state.lock().unwrap();
        if state.in_flight != Some(sequence) {
            return Err(AckError::Unexpected { sequence, in_flight: state.in_flight });
        }
        state.in_flight = None;
        self.freed.notify_all();
        Ok(())
    }

    /// Wakes any waiter for good; the desktop closed or we are shutting down.
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

/// Creates a memfd of `len` bytes sealed as upstream's (shrink, grow, seal).
pub fn create_memfd_of(len: usize) -> io::Result<OwnedFd> {
    use std::os::fd::FromRawFd;
    // SAFETY: a NUL-terminated name and valid flags.
    let fd = unsafe { libc::memfd_create(c"aera-plugin-desktop".as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING) };
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

    const SHORT: Duration = Duration::from_millis(1);

    fn tiny() -> Slots {
        let g = Geometry::desktop(4, 2);
        Slots::map(create_memfd(&g).unwrap(), g).unwrap()
    }

    #[test]
    fn sequence_starts_at_one_and_alternates_slots() {
        let slots = tiny();
        let mut seen = vec![];
        for _ in 0..4 {
            let f = slots.acquire(SHORT).unwrap();
            // SAFETY: held, not presented.
            unsafe { slots.slot_mut(&f).fill(f.sequence as u8) };
            let m = slots.present(&f);
            assert_eq!((m.kind, m.x, m.y), (play::FRAME, 4, 2));
            seen.push((m.sequence, f.index));
            slots.ack(m.sequence).unwrap();
        }
        assert_eq!(seen, vec![(1, 1), (2, 0), (3, 1), (4, 0)]);
    }

    #[test]
    fn one_frame_in_flight_until_its_ack() {
        let slots = tiny();
        let f = slots.acquire(SHORT).unwrap();
        assert!(!slots.has_free());
        assert!(slots.acquire(SHORT).is_none(), "the slot is held while written");
        let m = slots.present(&f);
        assert!(!slots.has_free());
        assert!(slots.acquire(SHORT).is_none(), "the next slot may still be on screen");
        assert_eq!(slots.ack(m.sequence + 1), Err(AckError::Unexpected { sequence: 2, in_flight: Some(1) }));
        slots.ack(m.sequence).unwrap();
        assert_eq!(slots.ack(m.sequence), Err(AckError::Unexpected { sequence: 1, in_flight: None }));
        assert!(slots.has_free());
        assert_eq!(slots.acquire(SHORT).map(|f| (f.sequence, f.index)), Some((2, 0)));
    }

    #[test]
    fn a_released_slot_keeps_its_sequence() {
        let slots = tiny();
        let f = slots.acquire(SHORT).unwrap();
        slots.release(&f);
        let again = slots.acquire(SHORT).unwrap();
        assert_eq!(again.sequence, 1);
        assert_eq!(slots.present(&again).sequence, 1);
    }

    #[test]
    fn ack_wakes_a_waiting_renderer() {
        let slots = std::sync::Arc::new(tiny());
        let f = slots.acquire(SHORT).unwrap();
        let m = slots.present(&f);
        let waiter = {
            let slots = slots.clone();
            std::thread::spawn(move || slots.acquire(Duration::from_secs(5)).map(|f| f.sequence))
        };
        std::thread::sleep(Duration::from_millis(20));
        slots.ack(m.sequence).unwrap();
        assert_eq!(waiter.join().unwrap(), Some(2));
    }

    #[test]
    fn close_ends_waits() {
        let slots = tiny();
        let f = slots.acquire(SHORT).unwrap();
        slots.present(&f);
        slots.close();
        assert!(slots.acquire(Duration::from_secs(5)).is_none());
        assert!(!slots.has_free());
    }

    #[test]
    fn sequence_wraps_without_skipping() {
        let slots = tiny();
        slots.state.lock().unwrap().next = u32::MAX;
        let f = slots.acquire(SHORT).unwrap();
        assert_eq!((f.sequence, f.index), (u32::MAX, 1));
        slots.ack(slots.present(&f).sequence).unwrap();
        assert_eq!(slots.acquire(SHORT).map(|f| (f.sequence, f.index)), Some((0, 0)));
    }

    #[test]
    fn refuses_small_or_unsealed_memory() {
        let g = Geometry::desktop(4, 2);
        assert!(Slots::map(create_memfd_of(g.total_bytes() - 1).unwrap(), g).is_err());
        use std::os::fd::FromRawFd;
        // SAFETY: a NUL-terminated name.
        let raw = unsafe { libc::memfd_create(c"unsealed".as_ptr(), libc::MFD_CLOEXEC) };
        // SAFETY: just created.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        // SAFETY: fd is a memfd.
        assert_eq!(unsafe { libc::ftruncate(fd.as_raw_fd(), g.total_bytes() as i64) }, 0);
        assert!(Slots::map(fd, g).is_err());
    }
}
