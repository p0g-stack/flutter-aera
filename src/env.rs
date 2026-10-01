//! Process environment for a stock app.
//!
//! `path_provider_linux` (and anything else following the XDG base
//! directory spec, as on GTK) finds its directories through `XDG_*`. AERA
//! gives each plugin a persistent directory (Host API 3's `AERA_PLUGIN_DATA`), so
//! the XDG homes point inside it. ALSA reads only the payload's own
//! configuration and plugins (`audio.rs`). Values already set are kept.

use std::path::Path;

pub const DIRS: [(&str, &str); 4] =
    [("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"), ("XDG_STATE_HOME", "state")];

/// The variables to set for `data`, skipping any `existing` says are set.
pub fn xdg_for(data: &Path, existing: impl Fn(&str) -> bool) -> Vec<(&'static str, std::path::PathBuf)> {
    DIRS.iter().filter(|(name, _)| !existing(name)).map(|(name, sub)| (*name, data.join(sub))).collect()
}

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
    for (name, path) in xdg_for(&data, |n| std::env::var_os(n).is_some()) {
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
    }
}
