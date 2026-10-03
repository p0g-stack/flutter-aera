//! Where the view's padding comes from. AERA's device tree sets the
//! status-bar indents at build time and exposes them nowhere at runtime
//! (`twrp get` and AERA's RPC cannot read variables), so the padding is
//! layered, per edge, first match wins:
//!
//! 1. `$AERA_PLUGIN_DATA/view.json`: the tuning override, edited in place
//!    over adb and read at the next start.
//! 2. `usr/share/flutter/view.json` in the payload: the app's own, which
//!    flutter_p0g writes from `aera/plugin.json`'s `"padding"`.
//! 3. What AERA gives us: the surface starts below AERA's status bar, so the
//!    top needs none.
//! 4. Our default, [`DEFAULT`]: the least that keeps controls off a phone's
//!    rounded corners.
//!
//! Both files are `{"padding": {"left": 4, "top": 0, "right": 4, "bottom": 8}}`
//! in logical pixels (dp); any edge may be left out to fall through.

use std::path::Path;

pub const FILE: &str = "view.json";
/// The payload's copy, relative to the payload root.
pub const PAYLOAD_FILE: &str = "usr/share/flutter/view.json";

/// Padding in logical pixels per edge; `None` falls through to the next
/// source.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Edges {
    pub left: Option<f64>,
    pub top: Option<f64>,
    pub right: Option<f64>,
    pub bottom: Option<f64>,
}

/// Our default, last: 4 dp at the sides, 8 dp at the bottom. On the
/// Infiniti the outer bottom-navigation labels end about 17 dp from the side
/// edges and were just cut by the rounded corners; 8 dp lifts them up the
/// curve. Minimal on purpose (Yuv).
pub const DEFAULT: Edges = Edges { left: Some(4.0), top: Some(0.0), right: Some(4.0), bottom: Some(8.0) };

/// What AERA's givens settle: the top. AERA draws its status bar above the
/// surface it hands us (on the Infiniti the 2772-pixel panel gives a
/// 2647-pixel surface), so nothing overlaps our top edge.
pub const GIVEN: Edges = Edges { left: None, top: Some(0.0), right: None, bottom: None };

impl Edges {
    /// `{"padding": {...}}`; a missing, negative or non-number edge is `None`.
    pub fn parse(text: &str) -> Result<Edges, String> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let p = v.get("padding").ok_or("no \"padding\" object")?;
        let edge = |k: &str| p.get(k).and_then(|x| x.as_f64()).filter(|x| x.is_finite() && *x >= 0.0);
        Ok(Edges { left: edge("left"), top: edge("top"), right: edge("right"), bottom: edge("bottom") })
    }

    /// The file at `path`, or empty if it is absent; a malformed file is
    /// logged and ignored.
    pub fn load(path: &Path) -> Edges {
        match std::fs::read_to_string(path) {
            Ok(text) => Edges::parse(&text).unwrap_or_else(|e| {
                eprintln!("aera-flutter: ignoring {}: {e}", path.display());
                Edges::default()
            }),
            Err(_) => Edges::default(),
        }
    }

    /// `self` where set, else `below`.
    pub fn over(self, below: Edges) -> Edges {
        Edges {
            left: self.left.or(below.left),
            top: self.top.or(below.top),
            right: self.right.or(below.right),
            bottom: self.bottom.or(below.bottom),
        }
    }

    /// [left, top, right, bottom] in logical pixels; unset edges are 0.
    pub fn dp(self) -> [f64; 4] {
        [self.left, self.top, self.right, self.bottom].map(|e| e.unwrap_or(0.0))
    }
}

/// The padding in logical pixels for the payload at `root`, from the four
/// sources above. Logs where each edge came from when a file set any.
pub fn resolve(root: &Path) -> [f64; 4] {
    let data = std::env::var_os(crate::host::DATA_ENV).map(|d| Edges::load(&Path::new(&d).join(FILE))).unwrap_or_default();
    let app = Edges::load(&root.join(PAYLOAD_FILE));
    let dp = layer(data, app);
    if data != Edges::default() || app != Edges::default() {
        eprintln!("aera-flutter: padding {dp:?} dp (override {data:?}, app {app:?})");
    }
    dp
}

/// Override over app over givens over default.
pub fn layer(data: Edges, app: Edges) -> [f64; 4] {
    data.over(app).over(GIVEN).over(DEFAULT).dp()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_no_files() {
        assert_eq!(layer(Edges::default(), Edges::default()), [4.0, 0.0, 4.0, 8.0]);
    }

    #[test]
    fn first_match_wins_per_edge() {
        let app = Edges::parse(r#"{"padding":{"left":6,"right":6,"bottom":12}}"#).unwrap();
        let data = Edges::parse(r#"{"padding":{"bottom":10}}"#).unwrap();
        assert_eq!(layer(data, app), [6.0, 0.0, 6.0, 10.0]);
        assert_eq!(layer(Edges::default(), app), [6.0, 0.0, 6.0, 12.0]);
        // A file may set the top: AERA's given is only the fallback.
        let top = Edges::parse(r#"{"padding":{"top":2}}"#).unwrap();
        assert_eq!(layer(top, Edges::default()), [4.0, 2.0, 4.0, 8.0]);
    }

    #[test]
    fn bad_edges_fall_through() {
        let e = Edges::parse(r#"{"padding":{"left":-1,"top":"x","right":3}}"#).unwrap();
        assert_eq!(e, Edges { right: Some(3.0), ..Edges::default() });
        assert!(Edges::parse("{}").is_err());
        assert!(Edges::parse("not json").is_err());
    }

    #[test]
    fn files_load_or_are_empty() {
        let dir = std::env::temp_dir().join(format!("padding-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(Edges::load(&dir.join("absent.json")), Edges::default());
        std::fs::write(dir.join("bad.json"), "{").unwrap();
        assert_eq!(Edges::load(&dir.join("bad.json")), Edges::default());
        std::fs::write(dir.join(FILE), r#"{"padding":{"left":5}}"#).unwrap();
        assert_eq!(Edges::load(&dir.join(FILE)).left, Some(5.0));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
