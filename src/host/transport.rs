//! The control socket: upstream's `SOCK_SEQPACKET` on fd 4, one [`Message`]
//! per packet. Every read goes through `recvmsg`, so descriptors attached to
//! a packet are always taken into [`OwnedFd`]s and closed unless the caller
//! keeps them.

use std::io;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::time::{Duration, Instant};

use super::{feature, kind, Message, MESSAGE_LEN, PROTOCOL_VERSION};

/// The most descriptors one packet may carry; `DESKTOP_READY` has two.
pub const MAX_FDS: usize = 2;

/// What the host told us in the handshake.
#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    /// `HELLO_ACK.flags`: the requested bits the host supports, plus
    /// metrics and back navigation.
    pub features: u32,
}

pub struct Control {
    fd: OwnedFd,
    /// Which direction we decode for: the plugin reads host kinds.
    from_host: bool,
}

/// Room for one more descriptor than allowed, so a packet with too many is
/// seen (and all of them closed) rather than cut by the kernel.
const CMSG_FDS: usize = MAX_FDS + 1;
#[repr(C)]
union Ancillary {
    _align: libc::cmsghdr,
    bytes: [u8; 64],
}

fn cmsg_space(fds: usize) -> usize {
    // SAFETY: a pure size computation.
    unsafe { libc::CMSG_SPACE((fds * std::mem::size_of::<RawFd>()) as u32) as usize }
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
        self.fd.as_fd()
    }

    /// Sends one message. Safe from any thread: each packet is one send.
    pub fn send(&self, message: &Message) -> io::Result<()> {
        self.send_with_fds(message, &[])
    }

    /// Sends one message with `fds` attached as one `SCM_RIGHTS` (the host
    /// side of `DESKTOP_READY`). The descriptors stay the caller's.
    pub fn send_with_fds(&self, message: &Message, fds: &[RawFd]) -> io::Result<()> {
        if fds.len() > MAX_FDS {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "too many descriptors for one packet"));
        }
        let bytes = message.encode();
        let mut iov = libc::iovec { iov_base: bytes.as_ptr() as *mut _, iov_len: MESSAGE_LEN };
        let mut ancillary = Ancillary { bytes: [0; 64] };
        // SAFETY: plain C struct; zeroed is valid.
        let mut header: libc::msghdr = unsafe { std::mem::zeroed() };
        header.msg_iov = &mut iov;
        header.msg_iovlen = 1;
        if !fds.is_empty() {
            let len = fds.len() * std::mem::size_of::<RawFd>();
            header.msg_control = (&mut ancillary as *mut Ancillary).cast();
            header.msg_controllen = cmsg_space(fds.len()) as _;
            // SAFETY: msg_control points at a buffer of msg_controllen
            // bytes, large enough for one header and `fds`.
            unsafe {
                let cmsg = libc::CMSG_FIRSTHDR(&header);
                (*cmsg).cmsg_level = libc::SOL_SOCKET;
                (*cmsg).cmsg_type = libc::SCM_RIGHTS;
                (*cmsg).cmsg_len = libc::CMSG_LEN(len as u32) as _;
                std::ptr::copy_nonoverlapping(fds.as_ptr().cast::<u8>(), libc::CMSG_DATA(cmsg), len);
            }
        }
        loop {
            // SAFETY: header and everything it points at live across the call.
            let sent = unsafe { libc::sendmsg(self.fd.as_raw_fd(), &header, libc::MSG_NOSIGNAL) };
            if sent == MESSAGE_LEN as isize {
                return Ok(());
            }
            if sent >= 0 {
                return Err(io::Error::new(io::ErrorKind::WriteZero, "short control packet"));
            }
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
        }
    }

    /// Reads one message, waiting up to `timeout` (`None` = forever).
    /// `Ok(None)` on timeout; `UnexpectedEof` when the peer is gone;
    /// `InvalidData` for a packet upstream `Valid()` rejects (size, magic,
    /// version, unterminated strings, direction), as upstream's `Session`
    /// fails on one. Descriptors on any packet read here are closed.
    pub fn recv(&self, timeout: Option<Duration>) -> io::Result<Option<Message>> {
        Ok(self.recv_with_fds(timeout)?.map(|(m, _)| m))
    }

    /// Reads one message if one is waiting; as [`Control::recv`].
    pub fn try_recv(&self) -> io::Result<Option<Message>> {
        Ok(self.try_recv_with_fds()?.map(|(m, _)| m))
    }

    /// As [`Control::recv`], but keeps the descriptors that came with the
    /// message, in the order sent. Also `InvalidData` when the kernel had to
    /// drop descriptors (`MSG_CTRUNC`) or a packet carried more than
    /// [`MAX_FDS`]. On every error whatever arrived is closed; the packet
    /// is consumed, so the caller decides whether the session goes on.
    pub fn recv_with_fds(&self, timeout: Option<Duration>) -> io::Result<Option<(Message, Vec<OwnedFd>)>> {
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            if let Some(m) = self.try_recv_with_fds()? {
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

    /// One packet if waiting, with its descriptors.
    pub fn try_recv_with_fds(&self) -> io::Result<Option<(Message, Vec<OwnedFd>)>> {
        let Some((bytes, n, fds)) = self.recv_packet()? else { return Ok(None) };
        let invalid = |what: String| Err(io::Error::new(io::ErrorKind::InvalidData, what));
        let Some(m) = (n == MESSAGE_LEN).then(|| Message::decode(&bytes[..n])).flatten() else {
            return invalid(format!("malformed control packet ({n} bytes, {} descriptors closed)", fds.len()));
        };
        let right_way = if self.from_host { kind::from_host(m.kind) } else { kind::from_plugin(m.kind) };
        if !right_way {
            return invalid(format!("control kind {} sent the wrong way ({} descriptors closed)", m.kind, fds.len()));
        }
        Ok(Some((m, fds)))
    }

    /// One raw packet: bytes, its true length (oversize shows), descriptors.
    fn recv_packet(&self) -> io::Result<Option<([u8; MESSAGE_LEN + 1], usize, Vec<OwnedFd>)>> {
        let mut bytes = [0u8; MESSAGE_LEN + 1];
        let mut ancillary = Ancillary { bytes: [0; 64] };
        debug_assert!(cmsg_space(CMSG_FDS) <= 64);
        loop {
            let mut iov = libc::iovec { iov_base: bytes.as_mut_ptr().cast(), iov_len: bytes.len() };
            // SAFETY: plain C struct; zeroed is valid.
            let mut header: libc::msghdr = unsafe { std::mem::zeroed() };
            header.msg_iov = &mut iov;
            header.msg_iovlen = 1;
            header.msg_control = (&mut ancillary as *mut Ancillary).cast();
            header.msg_controllen = cmsg_space(CMSG_FDS) as _;
            // SAFETY: header points at live buffers of the stated sizes.
            let n = unsafe {
                libc::recvmsg(self.fd.as_raw_fd(), &mut header, libc::MSG_DONTWAIT | libc::MSG_CMSG_CLOEXEC | libc::MSG_TRUNC)
            };
            if n < 0 {
                let e = io::Error::last_os_error();
                return match e.kind() {
                    io::ErrorKind::WouldBlock => Ok(None),
                    io::ErrorKind::Interrupted => continue,
                    _ => Err(e),
                };
            }
            // Take every descriptor first, so each error path closes them.
            let mut fds = vec![];
            // SAFETY: the kernel filled msg_control up to msg_controllen;
            // the CMSG_* walk stays within it, and each SCM_RIGHTS payload
            // is an array of descriptors now owned by this process.
            unsafe {
                let mut cmsg = libc::CMSG_FIRSTHDR(&header);
                while !cmsg.is_null() {
                    if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                        let data = libc::CMSG_DATA(cmsg);
                        let len = (*cmsg).cmsg_len as usize - (data as usize - cmsg as usize);
                        for i in 0..len / std::mem::size_of::<RawFd>() {
                            let fd = std::ptr::read_unaligned(data.cast::<RawFd>().add(i));
                            fds.push(OwnedFd::from_raw_fd(fd));
                        }
                    }
                    cmsg = libc::CMSG_NXTHDR(&header, cmsg);
                }
            }
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            if header.msg_flags & libc::MSG_CTRUNC != 0 || fds.len() > MAX_FDS {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "control packet carried too many descriptors"));
            }
            return Ok(Some((bytes, n as usize, fds)));
        }
    }

    /// The plugin's side of the handshake: `HELLO` asking for `features`
    /// → `HELLO_ACK` (version 2, must offer the desktop). Anything else that
    /// arrives first is returned so the caller can replay it.
    pub fn handshake(&self, features: u32, timeout: Duration) -> io::Result<(Session, Vec<Message>)> {
        self.send(&Message::hello(features))?;
        let deadline = Instant::now() + timeout;
        let mut early = vec![];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Some(m) = self.recv(Some(left))? else {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "no HELLO_ACK from the host"));
            };
            if m.kind != kind::HELLO_ACK {
                early.push(m);
                continue;
            }
            if m.value != PROTOCOL_VERSION {
                return Err(io::Error::new(io::ErrorKind::Unsupported, format!("host speaks protocol {}", m.value)));
            }
            if m.flags & feature::DESKTOP == 0 {
                return Err(io::Error::new(io::ErrorKind::Unsupported, "host offers no desktop surface"));
            }
            return Ok((Session { features: m.flags }, early));
        }
    }
}

/// A connected `SOCK_SEQPACKET` pair, for the control socket or the play
/// channel: (plugin end, host end).
pub fn pair() -> io::Result<(OwnedFd, OwnedFd)> {
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
    use crate::host::REQUESTED_FEATURES;

    /// A memfd under a name no other test uses.
    fn marked(name: &str) -> OwnedFd {
        let c = std::ffi::CString::new(name).unwrap();
        // SAFETY: a NUL-terminated name.
        let fd = unsafe { libc::memfd_create(c.as_ptr(), libc::MFD_CLOEXEC) };
        assert!(fd >= 0);
        // SAFETY: just created, owned by nothing else.
        unsafe { OwnedFd::from_raw_fd(fd) }
    }

    /// How many of this process's descriptors point at the memfd `name`;
    /// unaffected by tests running alongside.
    fn copies(name: &str) -> usize {
        let target = format!("/memfd:{name} ");
        std::fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|e| std::fs::read_link(e.ok()?.path()).ok())
            .filter(|l| format!("{} ", l.display()).starts_with(&target))
            .count()
    }

    /// Sends `bytes` with `fds` in one `SCM_RIGHTS`, bypassing every check.
    fn send_raw(fd: RawFd, bytes: &[u8], fds: &[RawFd]) {
        let mut iov = libc::iovec { iov_base: bytes.as_ptr() as *mut _, iov_len: bytes.len() };
        let mut anc = Ancillary { bytes: [0; 64] };
        // SAFETY: plain C struct; zeroed is valid.
        let mut h: libc::msghdr = unsafe { std::mem::zeroed() };
        h.msg_iov = &mut iov;
        h.msg_iovlen = 1;
        h.msg_control = (&mut anc as *mut Ancillary).cast();
        h.msg_controllen = cmsg_space(fds.len()) as _;
        let len = std::mem::size_of_val(fds);
        // SAFETY: as in send_with_fds; every buffer outlives the call.
        unsafe {
            let c = libc::CMSG_FIRSTHDR(&h);
            (*c).cmsg_level = libc::SOL_SOCKET;
            (*c).cmsg_type = libc::SCM_RIGHTS;
            (*c).cmsg_len = libc::CMSG_LEN(len as u32) as _;
            std::ptr::copy_nonoverlapping(fds.as_ptr().cast::<u8>(), libc::CMSG_DATA(c), len);
            assert_eq!(libc::sendmsg(fd, &h, 0), bytes.len() as isize);
        }
    }

    #[test]
    fn handshake_against_a_fake_host() {
        let (a, b) = pair().unwrap();
        let plugin = Control::plugin(a);
        let host = Control::host(b);
        let t = std::thread::spawn(move || {
            let hello = host.recv(Some(Duration::from_secs(5))).unwrap().unwrap();
            assert_eq!((hello.kind, hello.value, hello.flags), (kind::HELLO, 2, 2));
            assert_eq!(hello.request_id, Message::hello(REQUESTED_FEATURES).request_id);
            // Early messages before HELLO_ACK are kept for replay.
            host.send(&Message::with(kind::LIFECYCLE, 0, 1, 0)).unwrap();
            host.send(&Message::with(kind::HELLO_ACK, 0, PROTOCOL_VERSION, feature::DESKTOP | feature::METRICS)).unwrap();
            host
        });
        let (session, early) = plugin.handshake(REQUESTED_FEATURES, Duration::from_secs(5)).unwrap();
        let _host = t.join().unwrap();
        assert_eq!(session.features, feature::DESKTOP | feature::METRICS);
        assert_eq!(early, vec![Message::with(kind::LIFECYCLE, 0, 1, 0)]);
    }

    #[test]
    fn handshake_refuses_a_host_without_a_desktop() {
        let (a, b) = pair().unwrap();
        let plugin = Control::plugin(a);
        let host = Control::host(b);
        // An old host or an unmarked HELLO: the original flags 3.
        host.send(&Message::with(kind::HELLO_ACK, 0, PROTOCOL_VERSION, 3)).unwrap();
        assert_eq!(plugin.handshake(REQUESTED_FEATURES, Duration::from_secs(1)).unwrap_err().kind(), io::ErrorKind::Unsupported);
    }

    #[test]
    fn handshake_fails_on_a_wrong_way_packet() {
        let (a, b) = pair().unwrap();
        let plugin = Control::plugin(a);
        let host = Control::host(b);
        host.send(&Message::new(kind::OPEN_DESKTOP)).unwrap();
        assert_eq!(plugin.handshake(REQUESTED_FEATURES, Duration::from_secs(1)).unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn eof_when_the_host_goes() {
        let (a, b) = pair().unwrap();
        let plugin = Control::plugin(a);
        drop(b);
        assert_eq!(plugin.recv(Some(Duration::from_secs(1))).unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn descriptors_travel_in_order() {
        let (a, b) = pair().unwrap();
        let plugin = Control::plugin(a);
        let host = Control::host(b);
        let memfd = crate::host::surface::create_memfd_of(4096).unwrap();
        let (play_a, _play_b) = pair().unwrap();
        let ready = Message::with(kind::DESKTOP_READY, 9, 960, 540);
        host.send_with_fds(&ready, &[memfd.as_raw_fd(), play_a.as_raw_fd()]).unwrap();
        let (m, fds) = plugin.recv_with_fds(Some(Duration::from_secs(1))).unwrap().unwrap();
        assert_eq!(m, ready);
        assert_eq!(fds.len(), 2);
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        // SAFETY: open fd, valid out pointer.
        assert_eq!(unsafe { libc::fstat(fds[0].as_raw_fd(), &mut st) }, 0);
        assert_eq!(st.st_size, 4096);
        assert!(crate::host::play::check_seqpacket(fds[1].as_fd()).is_ok());
        // Received descriptors are close-on-exec.
        // SAFETY: open fd.
        assert_ne!(unsafe { libc::fcntl(fds[1].as_raw_fd(), libc::F_GETFD) } & libc::FD_CLOEXEC, 0);
    }

    #[test]
    fn bad_packets_are_errors_and_close_descriptors() {
        let (a, b) = pair().unwrap();
        let plugin = Control::plugin(a);
        let host = Control::host(b);
        let memfd = marked("control-test-bad");
        let invalid = |plugin: &Control| plugin.recv(Some(Duration::from_secs(1))).unwrap_err().kind();
        // A valid message read without asking for descriptors.
        host.send_with_fds(&Message::with(kind::LIFECYCLE, 0, 1, 0), &[memfd.as_raw_fd()]).unwrap();
        assert_eq!(plugin.recv(Some(Duration::from_secs(1))).unwrap().unwrap().value, 1);
        // A worker kind sent the wrong way, with a descriptor.
        host.send_with_fds(&Message::new(kind::CLOSE), &[memfd.as_raw_fd()]).unwrap();
        assert_eq!(invalid(&plugin), io::ErrorKind::InvalidData);
        // The wrong version, with a descriptor.
        let mut bad = Message::new(kind::LIFECYCLE).encode();
        bad[4] = 3;
        send_raw(host.raw_fd(), &bad, &[memfd.as_raw_fd()]);
        assert_eq!(invalid(&plugin), io::ErrorKind::InvalidData);
        // Truncated and oversized packets.
        let good = Message::new(kind::LIFECYCLE).encode();
        send_raw(host.raw_fd(), &good[..MESSAGE_LEN - 1], &[memfd.as_raw_fd()]);
        assert_eq!(invalid(&plugin), io::ErrorKind::InvalidData);
        let mut big = good.to_vec();
        big.push(0);
        send_raw(host.raw_fd(), &big, &[memfd.as_raw_fd()]);
        assert_eq!(invalid(&plugin), io::ErrorKind::InvalidData);
        // An unterminated string.
        let mut open = good;
        open[24 + crate::host::TITLE_LEN..].fill(b'a');
        // SAFETY: a valid buffer; fd open.
        assert_eq!(unsafe { libc::send(host.raw_fd(), open.as_ptr().cast(), open.len(), 0) }, MESSAGE_LEN as isize);
        assert_eq!(invalid(&plugin), io::ErrorKind::InvalidData);
        // Each error consumed only its packet; nothing leaked.
        host.send(&Message::with(kind::LIFECYCLE, 0, 3, 0)).unwrap();
        assert_eq!(plugin.recv(Some(Duration::from_secs(1))).unwrap().unwrap().value, 3);
        assert_eq!(copies("control-test-bad"), 1);
    }

    #[test]
    fn too_many_descriptors_is_an_error_and_closes_them() {
        let (a, b) = pair().unwrap();
        let plugin = Control::plugin(a);
        let host = Control::host(b);
        let memfd = marked("control-test-many");
        assert!(host.send_with_fds(&Message::new(kind::DESKTOP_READY), &[memfd.as_raw_fd(); 3]).is_err());
        // Bypass the sender's limit: four descriptors in one packet.
        send_raw(host.raw_fd(), &Message::new(kind::DESKTOP_READY).encode(), &[memfd.as_raw_fd(); 4]);
        assert_eq!(plugin.recv_with_fds(Some(Duration::from_secs(1))).unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert_eq!(copies("control-test-many"), 1);
        // The channel stays usable after the bad packet.
        host.send(&Message::with(kind::LIFECYCLE, 0, 1, 0)).unwrap();
        assert_eq!(plugin.recv(Some(Duration::from_secs(1))).unwrap().unwrap().value, 1);
    }
}
