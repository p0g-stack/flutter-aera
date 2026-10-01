//! AERA's own saved settings, read directly as root.
//!
//! AERA keeps its settings in TWRP's `InfoManager` file `.aera`: an `i32`
//! file version, then `[u16 length][name\0][u16 length][value\0]` pairs, all
//! native-endian (infomanager.cpp). There is no Host API for them and no
//! official plugin reads them; we only take what AERA has saved, so a
//! change still unsaved in this session is not seen.
//!
//! Where the file lives depends on AERA's build flags and on decryption
//! (variables.h, data.cpp): `/sdcard/AERA` by default, `/data/recovery/AERA`
//! with `OF_USE_DATA_RECOVERY_FOR_SETTINGS` or when decryption failed. The
//! newest readable one wins.

use std::collections::HashMap;
use std::path::Path;

/// `FILE_VERSION` in data.cpp; AERA ignores a file with any other.
pub const FILE_VERSION: i32 = 0x0001_0010;

pub const PATHS: [&str; 3] = ["/sdcard/AERA/.aera", "/data/media/0/AERA/.aera", "/data/recovery/AERA/.aera"];

/// AERA's accent when unset or invalid (aera_backend.cpp).
pub const DEFAULT_ACCENT: u32 = 0x16c8ff;

#[derive(Debug, Default, PartialEq)]
pub struct Settings(HashMap<String, String>);

impl Settings {
    /// Parses an `InfoManager` file. A wrong version or a truncated pair
    /// stops there, keeping what came before, as AERA does.
    pub fn parse(bytes: &[u8]) -> Option<Settings> {
        let version = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
        if version != FILE_VERSION {
            return None;
        }
        let mut rest = &bytes[4..];
        let mut values = HashMap::new();
        let field = |rest: &mut &[u8]| -> Option<String> {
            let len = u16::from_ne_bytes(rest.get(..2)?.try_into().ok()?) as usize;
            if len >= 512 {
                return None;
            }
            let raw = rest.get(2..2 + len)?;
            *rest = &rest[2 + len..];
            let text = raw.split(|b| *b == 0).next().unwrap_or_default();
            Some(String::from_utf8_lossy(text).into_owned())
        };
        while !rest.is_empty() {
            let Some(name) = field(&mut rest) else { break };
            let Some(value) = field(&mut rest) else { break };
            values.insert(name, value);
        }
        Some(Settings(values))
    }

    /// The newest of [`PATHS`] that parses, or empty settings.
    pub fn load() -> Settings {
        Self::load_from(&PATHS.map(Path::new))
    }

    pub fn load_from(paths: &[&Path]) -> Settings {
        paths
            .iter()
            .filter_map(|p| {
                let modified = std::fs::metadata(p).and_then(|m| m.modified()).ok()?;
                Some((modified, Settings::parse(&std::fs::read(p).ok()?)?))
            })
            .max_by_key(|(modified, _)| *modified)
            .map(|(_, s)| s)
            .unwrap_or_default()
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    /// `tw_military_time`: 1 is 24-hour. AERA's default is 12-hour.
    pub fn clock_24h(&self) -> bool {
        self.get("tw_military_time").and_then(|v| v.trim().parse::<i32>().ok()).unwrap_or(0) != 0
    }

    /// `aera_theme_accent` as `0xRRGGBB`, parsed as AERA does
    /// (`RecoveryAccentColor`: hex, non-zero, at most 24 bits), else AERA's
    /// default.
    pub fn accent(&self) -> u32 {
        self.get("aera_theme_accent")
            .map(|v| v.trim_start())
            .map(|v| v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")).unwrap_or(v))
            .and_then(|v| u32::from_str_radix(v, 16).ok())
            .filter(|rgb| (1..=0xff_ffff).contains(rgb))
            .unwrap_or(DEFAULT_ACCENT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn file(pairs: &[(&str, &str)]) -> Vec<u8> {
        let mut out = FILE_VERSION.to_ne_bytes().to_vec();
        for (n, v) in pairs {
            for s in [n, v] {
                out.extend_from_slice(&((s.len() + 1) as u16).to_ne_bytes());
                out.extend_from_slice(s.as_bytes());
                out.push(0);
            }
        }
        out
    }

    #[test]
    fn reads_infomanager_files() {
        let s = Settings::parse(&file(&[("tw_military_time", "1"), ("aera_theme_accent", "ff8800")])).unwrap();
        assert!(s.clock_24h());
        assert_eq!(s.accent(), 0xff8800);
        let mut bytes = file(&[("a", "1")]);
        bytes.extend_from_slice(&[9, 0, b'x']);
        assert_eq!(Settings::parse(&bytes).unwrap().get("a"), Some("1"));
        let mut other = file(&[]);
        other[0] ^= 1;
        assert!(Settings::parse(&other).is_none());
    }

    #[test]
    fn defaults_follow_aera() {
        let s = Settings::default();
        assert!(!s.clock_24h());
        assert_eq!(s.accent(), DEFAULT_ACCENT);
        let bad = Settings::parse(&file(&[("aera_theme_accent", "red"), ("tw_military_time", "0")])).unwrap();
        assert_eq!(bad.accent(), DEFAULT_ACCENT);
        assert!(!bad.clock_24h());
    }

    #[test]
    fn newest_file_wins() {
        let dir = std::env::temp_dir().join(format!("aera-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (old, new, missing) = (dir.join("old"), dir.join("new"), dir.join("missing"));
        std::fs::write(&old, file(&[("tw_military_time", "0")])).unwrap();
        std::fs::write(&new, file(&[("tw_military_time", "1")])).unwrap();
        let earlier = std::time::SystemTime::now() - std::time::Duration::from_secs(60);
        std::fs::File::options().write(true).open(&old).unwrap().set_modified(earlier).unwrap();
        assert!(Settings::load_from(&[&old, &missing, &new]).clock_24h());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
