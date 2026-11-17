//! The one view: window metrics from AERA's desktop surface and keyboard
//! inset. ≈ GTK's `fl_view`. Desktop input becomes pointer events in
//! `engine.rs`, from `host::input`.

use crate::ffi::{self, FlutterWindowMetricsEvent};
use crate::host::Geometry;

pub const VIEW_ID: i64 = 0;
pub const DISPLAY_ID: u64 = 0;

#[derive(Clone, Copy, Debug)]
pub struct View {
    pub geometry: Geometry,
    /// AERA's desktop keyboard height in surface pixels.
    pub bottom_inset: u32,
    /// The padding in logical pixels (left, top, right, bottom), from
    /// [`crate::padding::resolve`].
    pub padding_dp: [f64; 4],
}

impl View {
    pub fn new(geometry: Geometry) -> View {
        View { geometry, bottom_inset: 0, padding_dp: crate::padding::DEFAULT.dp() }
    }

    pub fn metrics(&self) -> FlutterWindowMetricsEvent {
        // SAFETY: a plain C struct; all-zero is a valid start.
        let mut m: FlutterWindowMetricsEvent = unsafe { std::mem::zeroed() };
        m.struct_size = std::mem::size_of::<FlutterWindowMetricsEvent>();
        m.width = self.geometry.width as usize;
        m.height = self.geometry.height as usize;
        m.pixel_ratio = self.geometry.scale;
        m.physical_view_inset_bottom = self.bottom_inset.min(self.geometry.height) as f64;
        // viewPadding, through third_party/flutter-engine patch 0001.
        let [left, top, right, bottom] = self.padding();
        m.physical_view_padding_left = left;
        m.physical_view_padding_top = top;
        m.physical_view_padding_right = right;
        m.physical_view_padding_bottom = bottom;
        m.display_id = DISPLAY_ID;
        m.view_id = VIEW_ID;
        m
    }

    /// The padding apps keep their controls out of, in physical pixels
    /// (left, top, right, bottom): `viewPadding`, sent with the window
    /// metrics (our engine patch 0001, as Android's embedding sets it).
    ///
    /// [`View::padding_dp`] at the surface's scale; see [`crate::padding`]
    /// for where it comes from.
    pub fn padding(&self) -> [f64; 4] {
        let scale = self.geometry.scale;
        self.padding_dp.map(|dp| (dp * scale).round())
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
        let mut v = View::new(Geometry::desktop(960, 540));
        v.bottom_inset = 300;
        let m = v.metrics();
        assert_eq!((m.width, m.height, m.pixel_ratio), (960, 540, 1.0));
        assert_eq!(m.physical_view_inset_bottom, 300.0);
        let p = v.padding();
        assert_eq!(
            [m.physical_view_padding_left, m.physical_view_padding_top, m.physical_view_padding_right, m.physical_view_padding_bottom],
            p
        );
    }

    #[test]
    fn padding_is_minimal() {
        // The Infiniti's portrait surface (0029) and its landscape one.
        let mut g = Geometry::desktop(960, 540);
        (g.width, g.height, g.scale) = (1272, 2647, 3.0);
        assert_eq!(View::new(g).padding(), [12.0, 0.0, 12.0, 24.0]);
        (g.width, g.height) = (2772, 1154);
        assert_eq!(View::new(g).padding(), [12.0, 0.0, 12.0, 24.0]);
        let mut v = View::new(g);
        v.padding_dp = [6.0, 0.0, 6.0, 10.0];
        assert_eq!(v.padding(), [18.0, 0.0, 18.0, 30.0]);
    }
}
