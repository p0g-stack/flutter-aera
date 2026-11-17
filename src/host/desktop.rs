//! Opening AERA's desktop surface: the setup page AERA shows behind it,
//! `OPEN_DESKTOP`, and checking `DESKTOP_READY` and its two descriptors.
//!
//! AERA keeps the plugin's setup page and lays the landscape desktop over
//! it. The desktop's EXIT returns there; the page's Open action asks for the
//! desktop again, Close ends the app.

use std::io;
use std::os::fd::{AsFd, OwnedFd};
use std::time::{Duration, Instant};

use super::play::PlayChannel;
use super::surface::{Geometry, Slots};
use super::transport::Control;
use super::{feature, kind, page, Message, DEFAULT_DESKTOP, DESKTOP_SIZES};

/// AERA's toolbar over the desktop's top left (EXIT, MENU, ESC, KEYS,
/// ENTER, TAB) takes touches with `x < 580` and `y < 65` in its 960x540
/// virtual layout, whatever the desktop's size.
pub const TOOLBAR: (u32, u32) = (580, 65);
const VIRTUAL_HEIGHT: u32 = 540;

/// How many desktop rows from the top AERA's toolbar covers.
pub fn toolbar_height(height: u32) -> u32 {
    (TOOLBAR.1 * height).div_ceil(VIRTUAL_HEIGHT)
}

/// Setup-page button: open the desktop again.
pub const ACTION_OPEN: u32 = 1;
/// Setup-page button: close the app.
pub const ACTION_CLOSE: u32 = 2;

/// The setup page: one `BEGIN_PAGE`…`COMMIT_PAGE` sequence.
pub fn setup_page(title: &str) -> Vec<Message> {
    vec![
        Message {
            title: title.into(),
            text: "The app runs on AERA's landscape desktop. EXIT in its toolbar returns here.".into(),
            ..Message::new(kind::BEGIN_PAGE)
        },
        Message { title: "Open desktop".into(), ..Message::with(kind::ADD_BUTTON, ACTION_OPEN, 0, page::PRIMARY) },
        Message { title: "Close".into(), ..Message::with(kind::ADD_BUTTON, ACTION_CLOSE, 0, page::DESTRUCTIVE) },
        Message::new(kind::COMMIT_PAGE),
    ]
}

pub fn status(text: &str) -> Message {
    Message { text: text.into(), ..Message::new(kind::SET_STATUS) }
}

/// What the desktop is asked for: `None` is upstream's default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    pub request_id: u32,
    pub size: Option<(u32, u32)>,
}

impl Request {
    /// Checks `size` against what the host offers; `None` asks for the
    /// default.
    pub fn new(request_id: u32, size: Option<(u32, u32)>, features: u32) -> Result<Request, String> {
        if request_id == 0 {
            return Err("OPEN_DESKTOP needs a nonzero request id".into());
        }
        match size {
            None => {}
            Some(s) if !DESKTOP_SIZES.contains(&s) => return Err(format!("{}x{} is not a desktop size AERA offers", s.0, s.1)),
            Some(s) if s != DEFAULT_DESKTOP && features & feature::DESKTOP_RESOLUTION == 0 => {
                return Err(format!("this AERA only offers the {}x{} desktop", DEFAULT_DESKTOP.0, DEFAULT_DESKTOP.1))
            }
            Some(_) => {}
        }
        Ok(Request { request_id, size })
    }

    /// The size `DESKTOP_READY` must report.
    pub fn expected(&self) -> (u32, u32) {
        self.size.unwrap_or(DEFAULT_DESKTOP)
    }

    pub fn message(&self) -> Message {
        let (w, h) = self.size.unwrap_or((0, 0));
        Message::with(kind::OPEN_DESKTOP, self.request_id, w, h)
    }
}

/// Sends `request` and waits up to `timeout` for its `DESKTOP_READY`.
/// Other messages that arrive meanwhile are returned for replay.
pub fn open(control: &Control, request: &Request, timeout: Duration) -> io::Result<(Desktop, Vec<Message>)> {
    control.send(&request.message())?;
    let deadline = Instant::now() + timeout;
    let mut early = vec![];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let Some((m, fds)) = control.recv_with_fds(Some(left))? else {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "no DESKTOP_READY from the host"));
        };
        if m.kind == kind::DESKTOP_READY {
            return Ok((Desktop::accept(request, &m, fds)?, early));
        }
        if !fds.is_empty() {
            eprintln!("aera-flutter: closing {} descriptors sent with kind {}", fds.len(), m.kind);
        }
        early.push(m);
    }
}

/// An open desktop: its frame memory and its play channel.
pub struct Desktop {
    pub request_id: u32,
    pub slots: Slots,
    pub play: PlayChannel,
}

impl Desktop {
    pub fn geometry(&self) -> Geometry {
        self.slots.geometry()
    }

    /// Checks a `DESKTOP_READY` for `request` and takes its descriptors.
    /// Every descriptor is closed on error. A refusal (zero size, no
    /// descriptors) is an error carrying AERA's reason.
    pub fn accept(request: &Request, m: &Message, fds: Vec<OwnedFd>) -> io::Result<Desktop> {
        let invalid = |what: String| io::Error::new(io::ErrorKind::InvalidData, what);
        if m.kind != kind::DESKTOP_READY || m.request_id != request.request_id {
            return Err(invalid(format!("DESKTOP_READY for request {}, expected {}", m.request_id, request.request_id)));
        }
        if m.value == 0 && m.flags == 0 {
            let reason = if m.text.is_empty() { "AERA refused the desktop".to_owned() } else { m.text.clone() };
            if !fds.is_empty() {
                return Err(invalid(format!("refused DESKTOP_READY carried {} descriptors", fds.len())));
            }
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, reason));
        }
        if (m.value, m.flags) != request.expected() {
            return Err(invalid(format!("desktop is {}x{}, asked for {:?}", m.value, m.flags, request.expected())));
        }
        let [frame, input]: [OwnedFd; 2] =
            fds.try_into().map_err(|f: Vec<OwnedFd>| invalid(format!("DESKTOP_READY carried {} descriptors, not 2", f.len())))?;
        // The input channel first: both are checked before anything maps.
        super::play::check_seqpacket(input.as_fd())?;
        let slots = Slots::map(frame, Geometry::desktop(m.value, m.flags))?;
        Ok(Desktop { request_id: request.request_id, slots, play: PlayChannel::new(input)? })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::surface::create_memfd;
    use crate::host::transport::pair;

    fn ready(id: u32, w: u32, h: u32) -> Message {
        Message::with(kind::DESKTOP_READY, id, w, h)
    }

    #[test]
    fn requests() {
        let r = Request::new(5, None, feature::DESKTOP).unwrap();
        assert_eq!((r.message().value, r.message().flags, r.expected()), (0, 0, (960, 540)));
        assert!(Request::new(0, None, feature::DESKTOP).is_err());
        assert!(Request::new(1, Some((1280, 720)), feature::DESKTOP).is_err());
        assert!(Request::new(1, Some((960, 540)), feature::DESKTOP).is_ok());
        assert!(Request::new(1, Some((1000, 500)), feature::DESKTOP | feature::DESKTOP_RESOLUTION).is_err());
        let r = Request::new(1, Some((1280, 720)), feature::DESKTOP | feature::DESKTOP_RESOLUTION).unwrap();
        assert_eq!((r.message().kind, r.message().value, r.message().flags), (kind::OPEN_DESKTOP, 1280, 720));
    }

    #[test]
    fn accepts_a_valid_desktop() {
        let r = Request::new(3, Some((640, 360)), feature::DESKTOP | feature::DESKTOP_RESOLUTION).unwrap();
        let g = Geometry::desktop(640, 360);
        let (input, _host) = pair().unwrap();
        let d = Desktop::accept(&r, &ready(3, 640, 360), vec![create_memfd(&g).unwrap(), input]).unwrap();
        assert_eq!(d.geometry(), g);
    }

    #[test]
    fn rejects_bad_desktops() {
        let r = Request::new(3, None, feature::DESKTOP).unwrap();
        let g = Geometry::desktop(960, 540);
        let fds = || vec![create_memfd(&g).unwrap(), pair().unwrap().0];
        assert!(Desktop::accept(&r, &ready(4, 960, 540), fds()).is_err(), "another request");
        assert!(Desktop::accept(&r, &ready(3, 1280, 720), fds()).is_err(), "another size");
        assert!(Desktop::accept(&r, &ready(3, 960, 540), vec![create_memfd(&g).unwrap()]).is_err(), "one fd");
        let swapped = { let mut f = fds(); f.swap(0, 1); f };
        assert!(Desktop::accept(&r, &ready(3, 960, 540), swapped).is_err(), "order");
        let small = vec![crate::host::surface::create_memfd_of(g.frame_bytes()).unwrap(), pair().unwrap().0];
        assert!(Desktop::accept(&r, &ready(3, 960, 540), small).is_err(), "one frame of memory");
        let refused = Message { text: "Desktop request unavailable or not permitted.".into(), ..ready(3, 0, 0) };
        let e = Desktop::accept(&r, &refused, vec![]).err().unwrap();
        assert_eq!((e.kind(), e.to_string()), (io::ErrorKind::PermissionDenied, refused.text.clone()));
        assert!(Desktop::accept(&r, &refused, fds()).is_err());
    }

    #[test]
    fn setup_page_is_one_sequence() {
        let p = setup_page("App");
        let kinds: Vec<u32> = p.iter().map(|m| m.kind).collect();
        assert_eq!(kinds, vec![kind::BEGIN_PAGE, kind::ADD_BUTTON, kind::ADD_BUTTON, kind::COMMIT_PAGE]);
        assert_eq!((p[1].request_id, p[2].request_id), (ACTION_OPEN, ACTION_CLOSE));
        assert!(p.iter().all(|m| m.encode().len() == crate::host::MESSAGE_LEN));
    }

    #[test]
    fn toolbar_scales_with_the_desktop() {
        assert_eq!(toolbar_height(540), 65);
        assert_eq!(toolbar_height(1080), 130);
        assert_eq!(toolbar_height(360), 44);
    }

    #[test]
    fn open_against_a_fake_host() {
        let (a, b) = pair().unwrap();
        let plugin = Control::plugin(a);
        let host = Control::host(b);
        let request = Request::new(7, None, feature::DESKTOP).unwrap();
        let t = std::thread::spawn(move || {
            let open = host.recv(Some(Duration::from_secs(5))).unwrap().unwrap();
            assert_eq!((open.kind, open.request_id, open.value, open.flags), (kind::OPEN_DESKTOP, 7, 0, 0));
            host.send(&Message::with(kind::LIFECYCLE, 0, 1, 0)).unwrap();
            let g = Geometry::desktop(960, 540);
            let memfd = create_memfd(&g).unwrap();
            let (input, host_input) = pair().unwrap();
            use std::os::fd::AsRawFd;
            host.send_with_fds(&ready(7, 960, 540), &[memfd.as_raw_fd(), input.as_raw_fd()]).unwrap();
            (host, PlayChannel::new(host_input).unwrap())
        });
        let (d, early) = open(&plugin, &request, Duration::from_secs(5)).unwrap();
        let (_host, host_play) = t.join().unwrap();
        assert_eq!(early, vec![Message::with(kind::LIFECYCLE, 0, 1, 0)]);
        // The first frame on the channel AERA keeps.
        let f = d.slots.acquire(Duration::from_millis(1)).unwrap();
        d.play.send(&d.slots.present(&f)).unwrap();
        let got = host_play.recv(Some(Duration::from_secs(1))).unwrap().unwrap();
        assert_eq!((got.kind, got.sequence, got.x, got.y), (crate::host::play::FRAME, 1, 960, 540));
    }
}
