//! Developer hooks, as GTK and flutter-pi have them.
//!
//! - Engine switches: GTK's `FLUTTER_ENGINE_SWITCHES=N` with
//!   `FLUTTER_ENGINE_SWITCH_1`… `_N`, plus, because AERA starts plugins with
//!   a fixed environment, one switch per line in
//!   `$AERA_PLUGIN_DATA/engine-switches` (`#` comments), which a developer
//!   writes over adb. For example `--vm-service-port=8181`.
//! - The VM service URL, which the engine only logs, is also written to
//!   `$AERA_PLUGIN_DATA/vm-service-url` so `flutter attach --debug-url`
//!   can find it over adb.

use std::path::{Path, PathBuf};

pub const SWITCHES_FILE: &str = "engine-switches";
pub const VM_SERVICE_FILE: &str = "vm-service-url";

/// GTK's `FLUTTER_ENGINE_SWITCHES` convention, read through `var`.
pub fn env_switches(var: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let count: usize = var("FLUTTER_ENGINE_SWITCHES").and_then(|n| n.trim().parse().ok()).unwrap_or(0);
    (1..=count).filter_map(|i| var(&format!("FLUTTER_ENGINE_SWITCH_{i}"))).collect()
}

/// One switch per line; blank lines and `#` comments are skipped.
pub fn file_switches(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(str::to_owned).collect()
}

fn data_dir() -> Option<PathBuf> {
    std::env::var_os(crate::host::DATA_ENV).map(PathBuf::from)
}

/// All developer switches for this process.
pub fn switches() -> Vec<String> {
    let mut out = env_switches(|k| std::env::var(k).ok());
    if let Some(text) = data_dir().and_then(|d| std::fs::read_to_string(d.join(SWITCHES_FILE)).ok()) {
        out.extend(file_switches(&text));
    }
    out
}

/// The URL in the engine's "The Dart VM service is listening on URL" line.
pub fn vm_service_url(message: &str) -> Option<&str> {
    let rest = message.split_once("Dart VM service is listening on ")?.1;
    rest.split_whitespace().next()
}

/// Called for every engine log line.
pub fn on_log(message: &str) {
    if let (Some(url), Some(dir)) = (vm_service_url(message), data_dir()) {
        write_url(&dir, url);
    }
}

fn write_url(dir: &Path, url: &str) {
    if let Err(e) = std::fs::write(dir.join(VM_SERVICE_FILE), format!("{url}\n")) {
        eprintln!("aera-flutter: cannot write {}: {e}", VM_SERVICE_FILE);
    }
}

/// Removes a URL left by an earlier run, so a stale one is never attached to.
pub fn clear_url() {
    if let Some(dir) = data_dir() {
        let _ = std::fs::remove_file(dir.join(VM_SERVICE_FILE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtk_switches() {
        let env = |k: &str| match k {
            "FLUTTER_ENGINE_SWITCHES" => Some("2".into()),
            "FLUTTER_ENGINE_SWITCH_1" => Some("--verbose-logging".into()),
            "FLUTTER_ENGINE_SWITCH_2" => Some("--vm-service-port=8181".into()),
            _ => None,
        };
        assert_eq!(env_switches(env), ["--verbose-logging", "--vm-service-port=8181"]);
        assert!(env_switches(|_| None).is_empty());
    }

    #[test]
    fn switch_file() {
        assert_eq!(file_switches("# dev\n--vm-service-port=8181\n\n  --disable-service-auth-codes \n"), [
            "--vm-service-port=8181",
            "--disable-service-auth-codes"
        ]);
    }

    #[test]
    fn url_from_the_log_line() {
        assert_eq!(
            vm_service_url("The Dart VM service is listening on http://127.0.0.1:8181/abc=/"),
            Some("http://127.0.0.1:8181/abc=/")
        );
        assert_eq!(vm_service_url("hello"), None);
    }
}
