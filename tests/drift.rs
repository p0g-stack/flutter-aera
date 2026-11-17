//! The upstream API 2 and opt-in extension wire definitions stay in lockstep.
//! Unmodified upstream headers are pinned at recovery 1cabc509.

use flutter_aera::host::{self, feature, kind, lifecycle};
use flutter_aera::host::Message;

fn header(name: &str) -> String {
    std::fs::read_to_string(format!("{}/vendor/aera/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// The value of `name = N` or `name = 1U << N` in a header.
fn value(text: &str, name: &str) -> u32 {
    let line = text
        .lines()
        .find(|l| l.split(|c: char| !c.is_alphanumeric() && c != '_').any(|w| w == name) && l.contains('='))
        .unwrap_or_else(|| panic!("{name} not in header"));
    let rhs = line.split('=').nth(1).unwrap();
    let rhs = rhs.split([',', ';', '/']).next().unwrap().trim();
    let number = |s: &str| -> u32 {
        let s = s.trim().trim_end_matches('U');
        if let Some(hex) = s.strip_prefix("0x") { u32::from_str_radix(hex, 16).unwrap() } else { s.parse().unwrap() }
    };
    match rhs.split_once("<<") {
        Some((a, b)) => number(a) << number(b),
        None => number(rhs),
    }
}

#[test]
fn host_api_2_numbers() {
    let h = header("protocol.hpp");
    assert_eq!(value(&h, "kMagic"), host::MAGIC);
    assert_eq!(value(&h, "kProtocolVersion"), host::PROTOCOL_VERSION);
    assert_eq!(Message::hello(0).request_id & 0xffff0000, value(&h, "kFeatureRequestMagic"));
    assert_eq!(value(&h, "kFeatureMetrics"), feature::METRICS);
    assert_eq!(value(&h, "kFeatureBackNavigation"), feature::BACK_NAVIGATION);
    assert_eq!(value(&h, "kHello"), kind::HELLO);
    assert_eq!(value(&h, "kHelloAck"), kind::HELLO_ACK);
    assert_eq!(value(&h, "kResume"), lifecycle::RESUME);
    assert!(h.contains(&format!("sizeof(Message) == {}", host::MESSAGE_LEN)));
    assert!(h.contains(&format!("title[{}]", host::TITLE_LEN)));
    assert!(h.contains(&format!("text[{}]", host::TEXT_LEN)));
    // Enum members without `=` count on from the previous one.
    let kinds = ["kHello", "kBeginPage", "kAddButton", "kCommitPage", "kSetStatus", "kRequestOperation", "kClose", "kAddSection", "kAddMetric", "kUpdateMetric", "kSetBackAction"];
    let at = |k: &str| kinds.iter().position(|x| *x == k).unwrap() as u32 + 1;
    assert_eq!(at("kSetStatus"), kind::SET_STATUS);
    assert_eq!(at("kClose"), kind::CLOSE);
    assert_eq!(at("kSetBackAction"), kind::SET_BACK_ACTION);
    for k in kinds {
        assert!(h.contains(k), "{k} missing from protocol.hpp");
    }
    let host_kinds = ["kHelloAck", "kAction", "kLifecycle", "kOperationResult"];
    for (i, k) in host_kinds.iter().enumerate() {
        assert!(h.contains(k));
        let n = kind::HELLO_ACK + i as u32;
        match *k {
            "kLifecycle" => assert_eq!(n, kind::LIFECYCLE),
            "kOperationResult" => assert_eq!(n, kind::OPERATION_RESULT),
            _ => {}
        }
    }
}

#[test]
fn desktop_wire_numbers() {
    use flutter_aera::host::play;
    let h = header("play_protocol.h");
    assert!(h.contains("#define AERA_PLAY_MAGIC 0x41575031u"));
    assert!(h.contains("uint32_t magic, kind, sequence; int32_t x, y;"));
    let body = &h[h.find("enum {").unwrap() + 6..h.find("};").unwrap()];
    let names: Vec<_> = body.split(',').map(|s| s.trim().split('=').next().unwrap()).collect();
    for (name, number) in [("PLAY_FRAME", play::FRAME), ("PLAY_ACK", play::ACK),
        ("PLAY_KEY", play::KEY), ("PLAY_BUTTON", play::BUTTON),
        ("PLAY_POINTER", play::POINTER), ("PLAY_LOOK", play::LOOK),
        ("PLAY_RELEASE", play::RELEASE), ("PLAY_CLOSE", play::CLOSE), ("PLAY_WHEEL", play::WHEEL)] {
        assert_eq!(names.iter().position(|s| *s == name).unwrap() as u32 + 1, number);
    }
}

/// The values of `enum class Kind`, counting members without `=` on from the
/// previous one.
fn kinds(text: &str) -> Vec<(String, u32)> {
    let body = &text[text.find("enum class Kind").unwrap()..];
    let body = &body[body.find('{').unwrap() + 1..body.find("};").unwrap()];
    let mut next = 0;
    let mut out = vec![];
    for line in body.lines() {
        let code = line.split("//").next().unwrap().trim().trim_end_matches(',');
        if code.is_empty() {
            continue;
        }
        let (name, n) = match code.split_once('=') {
            Some((name, n)) => (name.trim(), n.trim().parse().unwrap()),
            None => (code, next),
        };
        out.push((name.to_owned(), n));
        next = n + 1;
    }
    out
}

#[test]
fn desktop_and_extension_numbers() {
    let h = header("protocol_api2_extended.hpp");
    assert_eq!(value(&h, "kProtocolVersion"), 2);
    for (name, bit) in [
        ("kFeatureDesktop", feature::DESKTOP),
        ("kFeatureDesktopResolution", feature::DESKTOP_RESOLUTION),
        ("kFeatureClipboard", feature::CLIPBOARD),
        ("kFeatureTextInput", feature::TEXT_INPUT),
        ("kFeatureFilePicker", feature::FILE_PICKER),
        ("kFeatureDesktopIntegration", feature::DESKTOP_INTEGRATION),
        ("kFeatureDesktopTextInput", feature::DESKTOP_TEXT_INPUT),
    ] {
        assert_eq!(value(&h, name), bit, "{name}");
    }
    let all = kinds(&h);
    let of = |name: &str| all.iter().find(|(n, _)| n == name).unwrap().1;
    for (name, n) in [
        ("kOpenDesktop", kind::OPEN_DESKTOP),
        ("kDesktopReady", kind::DESKTOP_READY),
        ("kPickFile", kind::PICK_FILE),
        ("kFilePickerResult", kind::FILE_PICKER_RESULT),
        ("kSetDesktopKeyboard", kind::SET_DESKTOP_KEYBOARD),
        ("kDesktopKeyboardInset", kind::DESKTOP_KEYBOARD_INSET),
        ("kEditTextInput", kind::EDIT_TEXT_INPUT),
        ("kDismissTextInput", kind::DISMISS_TEXT_INPUT),
    ] {
        assert_eq!(of(name), n, "{name}");
    }
    // Extensions append values; no original upstream value may move.
    for (name, n) in kinds(&header("protocol.hpp")) {
        assert_eq!(of(&name), n, "upstream {name} was renumbered");
    }
}

/// The extended header is produced by the real recovery patch series.
#[test]
fn extended_header_is_the_patched_one() {
    let dir = env!("CARGO_MANIFEST_DIR");
    let vendored = header("protocol_api2_extended.hpp");
    let v2 = header("protocol.hpp");
    // Rebuild the header from Host API 2 and the series' protocol.hpp hunks.
    let mut text = v2;
    let series = std::fs::read_to_string(format!("{dir}/third_party/aera/patches-api2/series")).unwrap();
    for patch in series.lines().filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let p = std::fs::read_to_string(format!("{dir}/third_party/aera/patches-api2/{patch}")).unwrap();
        for file in p.split("diff --git ").skip(1) {
            if file.starts_with("a/aeraui/features/plugin_api/protocol.hpp ") {
                text = apply(&text, file);
            }
        }
    }
    assert_eq!(text, vendored);
}

/// Applies one file's unified-diff hunks (exact context, in order).
fn apply(text: &str, diff: &str) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = String::new();
    let mut at = 0;
    for hunk in diff.split("\n@@ ").skip(1) {
        let header = &hunk[..hunk.find(" @@").unwrap()];
        let old_start: usize = header[1..].split([',', ' ']).next().unwrap().parse().unwrap();
        let start = old_start.max(1) - 1;
        out.extend(lines[at..start].iter().copied());
        at = start;
        let hunk = format!("{hunk}\n");
        for line in hunk.split_inclusive('\n').skip(1) {
            if line.starts_with("-- \n") || line.starts_with("diff ") {
                break;
            }
            match line.as_bytes().first() {
                Some(b' ') => {
                    assert_eq!(lines[at], &line[1..]);
                    out.push_str(&line[1..]);
                    at += 1;
                }
                Some(b'-') => {
                    assert_eq!(lines[at], &line[1..]);
                    at += 1;
                }
                Some(b'+') => out.push_str(&line[1..]),
                _ => break,
            }
        }
    }
    out.extend(lines[at..].iter().copied());
    out
}

#[test]
fn vendored_headers_are_the_recorded_ones() {
    let pin = std::fs::read_to_string(format!("{}/spec/engine-pin.md", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let host = std::fs::read_to_string(format!("{}/spec/host.md", env!("CARGO_MANIFEST_DIR"))).unwrap();
    for (file, record) in [("vendor/flutter/flutter_embedder.h", &pin), ("vendor/aera/protocol.hpp", &host), ("vendor/aera/play_protocol.h", &host)] {
        let bytes = std::fs::read(format!("{}/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let out = std::process::Command::new("sha256sum").arg(format!("{}/{file}", env!("CARGO_MANIFEST_DIR"))).output().unwrap();
        let hash = String::from_utf8(out.stdout).unwrap().split_whitespace().next().unwrap().to_owned();
        assert!(!bytes.is_empty());
        assert!(record.contains(&hash), "{file} is {hash}, which the spec does not record");
    }
}
