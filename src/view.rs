//! The one view: window metrics from AERA's `SURFACE` and keyboard inset,
//! and touches as Flutter pointer events. ≈ GTK's `fl_view`.

use crate::ffi::{self, FlutterPointerEvent, FlutterWindowMetricsEvent};
use crate::handlers::window::Padding;
use crate::host::{kind, Geometry, Message};

pub const VIEW_ID: i64 = 0;
pub const DISPLAY_ID: u64 = 0;

/// The bottom padding, in logical pixels: on the Infiniti's rounded corners
/// labels 17 dp from the side were still cut, so 20 dp lifts them clear.
pub const CORNER_DP: f64 = 20.0;

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub geometry: Geometry,
    /// AERA's keyboard height in surface pixels.
    pub bottom_inset: u32,
}

impl View {
    pub fn new(geometry: Geometry) -> View {
        View { geometry, bottom_inset: 0 }
    }

    pub fn metrics(&self) -> FlutterWindowMetricsEvent {
        // SAFETY: a plain C struct; all-zero is a valid start.
        let mut m: FlutterWindowMetricsEvent = unsafe { std::mem::zeroed() };
        m.struct_size = std::mem::size_of::<FlutterWindowMetricsEvent>();
        m.width = self.geometry.width as usize;
        m.height = self.geometry.height as usize;
        m.pixel_ratio = self.geometry.scale;
        m.physical_view_inset_bottom = self.bottom_inset.min(self.geometry.height) as f64;
        m.display_id = DISPLAY_ID;
        m.view_id = VIEW_ID;
        m
    }

    /// The padding apps keep their controls out of, in physical pixels
    /// (left, top, right, bottom), sent on `aera/window`. Flutter's
    /// embedder API has no padding field (handlers/window.rs).
    ///
    /// Sides: half of AERA's edge Back zone, which is a twentieth of the
    /// screen's width (`max(72, width / 20)` on AERA's 1440-wide canvas,
    /// 0015), so a control is never deep in the zone. Bottom:
    /// [`CORNER_DP`], clear of a phone's rounded corners and of most of the
    /// bottom Recents strip. Top: none, AERA's status bar is above the view.
    /// Constants, not the device's real radius: AERA does not expose one
    /// (Yuv's compromise, 2026-10-02).
    pub fn padding(&self) -> Padding {
        let g = self.geometry;
        let side = (g.width as f64 / 40.0).round();
        let bottom = (CORNER_DP * g.scale).round().min(g.height as f64 / 4.0);
        [side, 0.0, side, bottom]
    }

    /// A touch from AERA as a Flutter pointer event, or `None` if `message`
    /// is not a touch.
    pub fn pointer(&self, message: &Message, timestamp_us: usize) -> Option<FlutterPointerEvent> {
        let phase = match message.kind {
            kind::TOUCH_DOWN => ffi::kDown,
            kind::TOUCH_MOVE => ffi::kMove,
            kind::TOUCH_UP => ffi::kUp,
            _ => return None,
        };
        // SAFETY: a plain C struct; all-zero is a valid start.
        let mut e: FlutterPointerEvent = unsafe { std::mem::zeroed() };
        e.struct_size = std::mem::size_of::<FlutterPointerEvent>();
        e.phase = phase;
        e.timestamp = timestamp_us;
        e.x = message.value.min(self.geometry.width.saturating_sub(1)) as f64;
        e.y = message.flags.min(self.geometry.height.saturating_sub(1)) as f64;
        e.device = message.request_id as i32;
        e.device_kind = ffi::kFlutterPointerDeviceKindTouch;
        e.view_id = VIEW_ID;
        Some(e)
    }

    /// The display, for `FlutterEngineNotifyDisplayUpdate` (frame pacing).
    pub fn display(&self) -> ffi::FlutterEngineDisplay {
        ffi::FlutterEngineDisplay {
            struct_size: std::mem::size_of::<ffi::FlutterEngineDisplay>(),
            display_id: DISPLAY_ID,
            single_display: true,
            refresh_rate: self.geometry.refresh_hz,
            width: self.geometry.width as usize,
            height: self.geometry.height as usize,
            device_pixel_ratio: self.geometry.scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_carry_the_keyboard_inset() {
        let mut v = View::new(Geometry::PHONE);
        v.bottom_inset = 900;
        let m = v.metrics();
        assert_eq!((m.width, m.height, m.pixel_ratio), (1080, 2400, 2.75));
        assert_eq!(m.physical_view_inset_bottom, 900.0);
    }

    #[test]
    fn padding_is_half_the_edge_zone_and_a_corner() {
        // The Infiniti's portrait surface (0029) and its landscape one.
        let mut g = Geometry::PHONE;
        (g.width, g.height, g.scale) = (1272, 2647, 3.0);
        assert_eq!(View::new(g).padding(), [32.0, 0.0, 32.0, 60.0]);
        (g.width, g.height) = (2772, 1154);
        assert_eq!(View::new(g).padding(), [69.0, 0.0, 69.0, 60.0]);
    }

    #[test]
    fn touches_become_pointer_events() {
        let v = View::new(Geometry::PHONE);
        let e = v.pointer(&Message::with(kind::TOUCH_DOWN, 2, 100, 5000), 7).unwrap();
        assert_eq!((e.phase, e.device, e.x, e.y, e.timestamp), (ffi::kDown, 2, 100.0, 2399.0, 7));
        assert!(v.pointer(&Message::new(kind::BACK), 0).is_none());
    }
}
