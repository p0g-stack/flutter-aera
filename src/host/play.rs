//! The desktop input/frame channel (`components/play_protocol.h`): a
//! `SOCK_SEQPACKET` socket from `DESKTOP_READY`, one 20-byte [`PlayMessage`]
//! per packet.
//!
//! Worker to host: only [`FRAME`], `sequence` exactly one more than the last
//! frame, `x`/`y` the desktop's width/height; the frame is in slot
//! `sequence % 2`. Host to worker: [`ACK`] for the last frame it latched
//! (`sequence`), and input. Every host packet's `sequence` is the last frame
//! it received. The host closes the channel on any bad frame.

use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd, RawFd};
use std::time::{Duration, Instant};

/// `AERA_PLAY_MAGIC`.
pub const MAGIC: u32 = 0x4157_5031;
/// `sizeof(PlayMessage)`.
pub const MESSAGE_LEN: usize = 20;

pub const FRAME: u32 = 1;
pub const ACK: u32 = 2;
/// `x` an X11 keysym, `y` 1 down / 0 up.
pub const KEY: u32 = 3;
/// `x` the button (1 left, 2 middle, 3 right), `y` 1 down / 0 up.
pub const BUTTON: u32 = 4;
/// `x`/`y` in desktop pixels.
pub const POINTER: u32 = 5;
/// Relative motion in the host's game mode.
pub const LOOK: u32 = 6;
/// Every held button and key is up (mode switch, keyboard, suspend, exit).
pub const RELEASE: u32 = 7;
/// The desktop is closing.
pub const CLOSE: u32 = 8;
/// `x` wheel steps, clamped to ±8.
pub const WHEEL: u32 = 9;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayMessage {
    pub kind: u32,
    pub sequence: u32,
    pub x: i32,
    pub y: i32,
}

impl PlayMessage {
    pub fn new(kind: u32, sequence: u32, x: i32, y: i32) -> PlayMessage {
        PlayMessage { kind, sequence, x, y }
    }

    pub fn encode(&self) -> [u8; MESSAGE_LEN] {
        let mut out = [0u8; MESSAGE_LEN];
        let words = [MAGIC, self.kind, self.sequence, self.x as u32, self.y as u32];
        for (i, word) in words.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        out
    }

    /// `None` unless exactly 20 bytes with the magic and a known kind.
    pub fn decode(bytes: &[u8]) -> Option<PlayMessage> {
        if bytes.len() != MESSAGE_LEN {
            return None;
        }
        let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
        if word(0) != MAGIC || !(FRAME..=WHEEL).contains(&word(1)) {
            return None;
        }
        Some(PlayMessage { kind: word(1), sequence: word(2), x: word(3) as i32, y: word(4) as i32 })
    }
}

/// Fails unless `fd` is a `SOCK_SEQPACKET` socket.
pub fn check_seqpacket(fd: BorrowedFd<'_>) -> io::Result<()> {
    let mut kind: libc::c_int = 0;
    let mut len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    // SAFETY: valid out pointers of the right size; fd is open.
    let r = unsafe { libc::getsockopt(fd.as_raw_fd(), libc::SOL_SOCKET, libc::SO_TYPE, (&mut kind as *mut libc::c_int).cast(), &mut len) };
    if r != 0 {
        return Err(io::Error::last_os_error());
    }
    if kind != libc::SOCK_SEQPACKET {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "not a SOCK_SEQPACKET socket"));
    }
    Ok(())
}

/// One end of the play channel. Safe to share: each packet is one syscall.
pub struct PlayChannel {
    fd: OwnedFd,
}

impl PlayChannel {
    pub fn new(fd: OwnedFd) -> io::Result<PlayChannel> {
        check_seqpacket(fd.as_fd())?;
        Ok(PlayChannel { fd })
    }

    /// Sends one packet without blocking; a full or closed channel is an
    /// error (the host never silently loses a packet either).
    pub fn send(&self, message: &PlayMessage) -> io::Result<()> {
        let bytes = message.encode();
        loop {
            // SAFETY: a valid buffer; fd is open.
            let sent = unsafe {
                libc::send(self.fd.as_raw_fd(), bytes.as_ptr().cast(), MESSAGE_LEN, libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL)
            };
            if sent == MESSAGE_LEN as isize {
                return Ok(());
            }
            if sent >= 0 {
                return Err(io::Error::new(io::ErrorKind::WriteZero, "short play packet"));
            }
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
        }
    }

    /// Reads one packet, waiting up to `timeout` (`None` = forever).
    /// `Ok(None)` on timeout, `UnexpectedEof` when the peer is gone,
    /// `InvalidData` for a malformed packet.
    pub fn recv(&self, timeout: Option<Duration>) -> io::Result<Option<PlayMessage>> {
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            if let Some(m) = self.try_recv()? {
                return Ok(Some(m));
            }
            let wait = match deadline {
                None => -1,
                Some(d) => {
                    let left = d.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Ok(None);
                    }
                    left.as_millis().clamp(1, i32::MAX as u128) as i32
                }
            };
            let mut pfd = libc::pollfd { fd: self.fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            // SAFETY: one valid pollfd.
            if unsafe { libc::poll(&mut pfd, 1, wait) } < 0 {
                let e = io::Error::last_os_error();
                if e.kind() != io::ErrorKind::Interrupted {
                    return Err(e);
                }
            }
        }
    }

    /// Reads one packet if one is waiting.
    pub fn try_recv(&self) -> io::Result<Option<PlayMessage>> {
        let mut bytes = [0u8; MESSAGE_LEN + 1];
        loop {
            // SAFETY: a valid buffer; fd is open.
            let n = unsafe {
                libc::recv(self.fd.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len(), libc::MSG_DONTWAIT | libc::MSG_TRUNC)
            };
            if n < 0 {
                let e = io::Error::last_os_error();
                return match e.kind() {
                    io::ErrorKind::WouldBlock => Ok(None),
                    io::ErrorKind::Interrupted => continue,
                    _ => Err(e),
                };
            }
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            // MSG_TRUNC: `n` is the real length, so oversize packets fail.
            let n = n as usize;
            return match (n <= MESSAGE_LEN).then(|| PlayMessage::decode(&bytes[..n])).flatten() {
                Some(m) => Ok(Some(m)),
                None => Err(io::Error::new(io::ErrorKind::InvalidData, format!("bad play packet ({n} bytes)"))),
            };
        }
    }

    pub fn raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl AsRawFd for PlayChannel {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl AsFd for PlayChannel {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.fd.as_fd()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::transport::pair;

    #[test]
    fn layout_is_the_c_struct() {
        let m = PlayMessage::new(POINTER, 7, -3, 540);
        let b = m.encode();
        assert_eq!(&b[0..4], &MAGIC.to_le_bytes());
        assert_eq!(&b[4..8], &POINTER.to_le_bytes());
        assert_eq!(&b[8..12], &7u32.to_le_bytes());
        assert_eq!(&b[12..16], &(-3i32).to_le_bytes());
        assert_eq!(PlayMessage::decode(&b), Some(m));
    }

    #[test]
    fn rejects_bad_packets() {
        let mut b = PlayMessage::new(ACK, 1, 0, 0).encode();
        assert!(PlayMessage::decode(&b[..19]).is_none());
        b[4] = 0;
        assert!(PlayMessage::decode(&b).is_none());
        b[4] = 10;
        assert!(PlayMessage::decode(&b).is_none());
        b[4] = 2;
        b[0] ^= 1;
        assert!(PlayMessage::decode(&b).is_none());
    }

    #[test]
    fn channel_round_trip_and_errors() {
        let (a, b) = pair().unwrap();
        let (a, b) = (PlayChannel::new(a).unwrap(), PlayChannel::new(b).unwrap());
        assert_eq!(a.recv(Some(Duration::from_millis(1))).unwrap(), None);
        b.send(&PlayMessage::new(ACK, 4, 0, 0)).unwrap();
        assert_eq!(a.recv(Some(Duration::from_secs(1))).unwrap(), Some(PlayMessage::new(ACK, 4, 0, 0)));
        // An oversized packet is not silently cut to 20 bytes.
        let big = [0u8; 24];
        // SAFETY: a valid buffer; fd open.
        unsafe { libc::send(b.raw_fd(), big.as_ptr().cast(), big.len(), 0) };
        assert_eq!(a.recv(Some(Duration::from_secs(1))).unwrap_err().kind(), io::ErrorKind::InvalidData);
        drop(b);
        assert_eq!(a.recv(Some(Duration::from_secs(1))).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn refuses_a_stream_socket() {
        use std::os::fd::FromRawFd;
        let mut fds = [0; 2];
        // SAFETY: a valid out array.
        assert_eq!(unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0, fds.as_mut_ptr()) }, 0);
        // SAFETY: just created, owned by nothing else.
        let (a, _b) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        assert!(PlayChannel::new(a).is_err());
    }
}
