//! Process environment for a stock app.
//!
//! `path_provider_linux` (and anything else following the XDG base
//! directory spec, as on GTK) finds its directories through `XDG_*`. AERA
//! gives each plugin a persistent directory (Host API 3's `AERA_PLUGIN_DATA`), so
//! the XDG homes point inside it, and so does `HOME` (AERA sets `/tmp`).
//! Temporary files go to a folder of the app's own in recovery's RAM `/tmp`
//! (AERA's `TMPDIR` is `/tmp` itself, shared with AERA and every plugin).
//! Documents and Downloads come from the payload's `xdg-user-dir`
//! (`launcher/xdg-user-dir`). ALSA reads only the payload's own configuration and
//! plugins (`audio.rs`). Values already set are kept, except AERA's `/tmp`.
//! `spec/aerap.md`, Storage and lifecycle, has the whole mapping.

use std::path::Path;

pub const DIRS: [(&str, &str); 4] =
    [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"), ("XDG_STATE_HOME", "state")];

/// The variables to set for `data`, skipping any `existing` says are set.
pub fn xdg_for(data: &Path, existing: impl Fn(&str) -> bool) -> Vec<(&'static str, std::path::PathBuf)> {
    DIRS.iter().filter(|(name, _)| !existing(name)).map(|(name, sub)| (*name, data.join(sub))).collect()
}

/// `HOME` and `TMPDIR` for `data`, unless `current` says they are set to
/// something other than AERA's `/tmp`. The temporary folder is named after
/// the plugin id (the data directory's name).
pub fn home_for(data: &Path, current: impl Fn(&str) -> Option<String>) -> Vec<(&'static str, std::path::PathBuf)> {
    let id = data.file_name().unwrap_or_default();
    [("HOME", data.to_path_buf()), ("TMPDIR", Path::new(TMP_ROOT).join(id))]
        .into_iter()
        .filter(|(name, _)| current(name).is_none_or(|v| v == "/tmp"))
        .collect()
}

/// Recovery's RAM, cleared at every boot.
pub const TMP_ROOT: &str = "/tmp/aera-flutter";

/// ALSA's configuration and plugin directory in the payload at `root`,
/// skipping any `existing` says are set.
pub fn alsa_for(root: &Path, existing: impl Fn(&str) -> bool) -> Vec<(&'static str, std::path::PathBuf)> {
    [("ALSA_CONFIG_PATH", "usr/share/alsa/alsa.conf"), ("ALSA_PLUGIN_DIR", "usr/lib/alsa-lib")]
        .into_iter()
        .filter(|(name, _)| !existing(name))
        .map(|(name, sub)| (name, root.join(sub)))
        .collect()
}

/// Applies [`alsa_for`] and [`xdg_for`] to this process. Call before any
/// thread starts.
pub fn apply(root: &Path) {
    for (name, path) in alsa_for(root, |n| std::env::var_os(n).is_some()) {
        // SAFETY (set_var): called from main before any other thread exists.
        unsafe { std::env::set_var(name, path) };
    }
    let Some(data) = std::env::var_os(crate::host::DATA_ENV) else {
        eprintln!("aera-flutter: {} is not set; XDG directories left as they are", crate::host::DATA_ENV);
        return;
    };
    let data = std::path::PathBuf::from(data);
    let current = |n: &str| std::env::var_os(n).map(|v| v.to_string_lossy().into_owned());
    let dirs = home_for(&data, current).into_iter().chain(xdg_for(&data, |n| std::env::var_os(n).is_some()));
    for (name, path) in dirs {
        let _ = std::fs::create_dir_all(&path);
        // SAFETY (set_var): called from main before the engine or any other
        // thread exists.
        unsafe { std::env::set_var(name, path) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_existing_values() {
        let v = xdg_for(Path::new("/d"), |n| n == "XDG_CACHE_HOME");
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], ("XDG_DATA_HOME", "/d/data".into()));
        assert!(v.iter().all(|(n, _)| *n != "XDG_CACHE_HOME"));
        let a = alsa_for(Path::new("/p"), |n| n == "ALSA_PLUGIN_DIR");
        assert_eq!(a, [("ALSA_CONFIG_PATH", "/p/usr/share/alsa/alsa.conf".into())]);
        let aera = |n: &str| Some(if n == "HOME" { "/tmp" } else { "/mine" }.to_owned());
        assert_eq!(home_for(Path::new("/sdcard/AERA/plugin-data/x"), aera), [("HOME", "/sdcard/AERA/plugin-data/x".into())]);
        assert_eq!(home_for(Path::new("/d/x"), |_| None)[1], ("TMPDIR", "/tmp/aera-flutter/x".into()));
    }
}
