//! The control socket: Host API 2's `SOCK_SEQPACKET` on fd 4, one
//! [`Message`] per packet.

use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, OwnedFd, RawFd};
use std::time::{Duration, Instant};

use super::{feature, kind, Geometry, Message, MESSAGE_LEN};

/// What the host told us in the handshake.
#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    pub features: u32,
    pub geometry: Geometry,
}

pub struct Control {
    fd: OwnedFd,
    /// Which direction we decode for: the plugin reads host kinds.
    from_host: bool,
}

impl Control {
    /// The plugin's end.
    pub fn plugin(fd: OwnedFd) -> Control {
        Control { fd, from_host: true }
    }

    /// The host's end (the simulator, tests).
    pub fn host(fd: OwnedFd) -> Control {
        Control { fd, from_host: false }
    }

    pub fn raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }

    pub fn as_fd(&self) -> BorrowedFd<'_> {
        use std::os::fd::AsFd;
        self.fd.as_fd()
    }

    /// Sends one message. Safe from any thread: each packet is one send.
    pub fn send(&self, message: &Message) -> io::Result<()> {
        let bytes = message.encode();
        // SAFETY: bytes is a valid buffer of MESSAGE_LEN bytes; fd is open.
        let sent = unsafe { libc::send(self.fd.as_raw_fd(), bytes.as_ptr().cast(), MESSAGE_LEN, libc::MSG_NOSIGNAL) };
        if sent < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Reads one message, waiting up to `timeout` (`None` = forever).
    /// `Ok(None)` on timeout; `UnexpectedEof` when the peer is gone. Packets
    /// that are malformed or travel the wrong way are dropped, as Host API 2
    /// does.
    pub fn recv(&self, timeout: Option<Duration>) -> io::Result<Option<Message>> {
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            let wait = match deadline {
                None => -1,
                Some(d) => d.saturating_duration_since(Instant::now()).as_millis().min(i32::MAX as u128) as i32,
            };
            let mut pfd = libc::pollfd { fd: self.fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            // SAFETY: one valid pollfd.
            let ready = unsafe { libc::poll(&mut pfd, 1, wait) };
            if ready < 0 {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(e);
            }
            if ready == 0 {
                return Ok(None);
            }
            match self.try_recv()? {
                Some(m) => return Ok(Some(m)),
                None if pfd.revents & (libc::POLLHUP | libc::POLLERR) != 0 => {
                    return Err(io::ErrorKind::UnexpectedEof.into())
                }
                None => {}
            }
        }
    }

    /// Reads one message if one is waiting.
    pub fn try_recv(&self) -> io::Result<Option<Message>> {
        let mut bytes = [0u8; MESSAGE_LEN + 1];
        loop {
            // SAFETY: bytes is a valid buffer; fd is open.
            let n = unsafe {
                libc::recv(self.fd.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len(), libc::MSG_DONTWAIT)
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
            let Some(m) = Message::decode(&bytes[..n as usize]) else { continue };
            let right_way = if self.from_host { kind::from_host(m.kind) } else { kind::from_plugin(m.kind) };
            if right_way {
                return Ok(Some(m));
            }
        }
    }

    /// The plugin's side of the handshake: `HELLO` → `HELLO_ACK` (must
    /// offer the pixel surface) → `SURFACE`. Anything else that arrives
    /// first is returned so the caller can replay it.
    pub fn handshake(&self, timeout: Duration) -> io::Result<(Session, Vec<Message>)> {
        self.send(&Message::hello())?;
        let deadline = Instant::now() + timeout;
        let mut features = None;
        let mut early = vec![];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Some(m) = self.recv(Some(left))? else {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "no HELLO_ACK and SURFACE from the host"));
            };
            match m.kind {
                kind::HELLO_ACK => {
                    if m.value & feature::PIXEL_SURFACE == 0 {
                        return Err(io::Error::new(io::ErrorKind::Unsupported, "host has no pixel surface"));
                    }
                    features = Some(m.value);
                }
                kind::SURFACE if features.is_some() => {
                    let geometry = Geometry::from_message(&m)
                        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad SURFACE"))?;
                    return Ok((Session { features: features.unwrap(), geometry }, early));
                }
                _ => early.push(m),
            }
        }
    }
}

/// A connected `SOCK_SEQPACKET` pair: (plugin end, host end).
pub fn pair() -> io::Result<(OwnedFd, OwnedFd)> {
    use std::os::fd::FromRawFd;
    let mut fds = [0; 2];
    // SAFETY: fds is a valid out array of two ints.
    if unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC, 0, fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: both were just created and are owned by nothing else.
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_against_a_fake_host() {
        let (a, b) = pair().unwrap();
        let plugin = Control::plugin(a);
        let host = Control::host(b);
        let t = std::thread::spawn(move || {
            let hello = host.recv(Some(Duration::from_secs(5))).unwrap().unwrap();
            assert_eq!((hello.kind, hello.value, hello.flags), (kind::HELLO, 3, 3));
            // A host-bound kind sent the wrong way is dropped.
            host.send(&Message::new(kind::PRESENT)).unwrap();
            host.send(&Message::with(kind::HELLO_ACK, 0, feature::PIXEL_SURFACE | feature::BACK_NAVIGATION, 0)).unwrap();
            host.send(&Message::with(kind::LIFECYCLE, 0, 1, 0)).unwrap();
            host.send(&Geometry::PHONE.to_message()).unwrap();
            host
        });
        let (session, early) = plugin.handshake(Duration::from_secs(5)).unwrap();
        let _host = t.join().unwrap();
        assert_eq!(session.geometry, Geometry::PHONE);
        assert_eq!(early, vec![Message::with(kind::LIFECYCLE, 0, 1, 0)]);
    }

    #[test]
    fn handshake_refuses_a_host_without_pixels() {
        let (a, b) = pair().unwrap();
        let plugin = Control::plugin(a);
        let host = Control::host(b);
        host.send(&Message::with(kind::HELLO_ACK, 0, feature::METRICS, 0)).unwrap();
        let e = plugin.handshake(Duration::from_secs(1)).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::Unsupported);
    }

    #[test]
    fn eof_when_the_host_goes() {
        let (a, b) = pair().unwrap();
        let plugin = Control::plugin(a);
        drop(b);
        assert_eq!(plugin.recv(Some(Duration::from_secs(1))).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }
}
