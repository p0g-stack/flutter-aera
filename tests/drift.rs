//! The drift gate: the numbers in `src/host/` match the vendored headers
//! (`vendor/aera/protocol.hpp` from AERA, `protocol_v3.hpp` from our Host
//! API 3 patches).

use flutter_aera::host::{self, feature, kind, lifecycle};

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

/// `name = N` anywhere in a header (several may share a line).
fn assigned(text: &str, name: &str) -> u32 {
    let at = text.find(&format!("{name} = ")).unwrap_or_else(|| panic!("{name} not in header"));
    let rest = &text[at + name.len() + 3..];
    rest[..rest.find(|c: char| !c.is_ascii_digit()).unwrap()].parse().unwrap()
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
fn host_api_3_numbers() {
    let h = header("protocol_v3.hpp");
    assert_eq!(value(&h, "kProtocolVersion3"), host::PROTOCOL_VERSION);
    assert_eq!(value(&h, "kSurfaceFd") as i32, host::SURFACE_FD);
    assert_eq!(value(&h, "kFeaturePixelSurface"), feature::PIXEL_SURFACE);
    assert_eq!(value(&h, "kFeatureKeyboardInset"), feature::KEYBOARD_INSET);
    assert_eq!(value(&h, "kFeatureFilePicker"), feature::FILE_PICKER);
    assert_eq!(value(&h, "kPickFiles"), host::operation::PICK_FILES);
    assert_eq!(assigned(&h, "kFile"), host::pick_mode::FILE);
    assert!(h.contains("enum class PickMode : uint32_t { kFile = 0, kFiles, kFolder, kSave };"));
    assert_eq!((host::pick_mode::FILES, host::pick_mode::FOLDER, host::pick_mode::SAVE), (1, 2, 3));
    assert_eq!(assigned(&h, "kText"), host::purpose::TEXT);
    assert_eq!(assigned(&h, "kDigits"), host::purpose::DIGITS);
    let all = kinds(&h);
    let of = |name: &str| all.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name}")).1;
    for (name, n) in [
        ("kHello", kind::HELLO),
        ("kRequestOperation", kind::REQUEST_OPERATION),
        ("kSetBackAction", kind::SET_BACK_ACTION),
        ("kPresent", kind::PRESENT),
        ("kKeyboardShow", kind::KEYBOARD_SHOW),
        ("kKeyboardHide", kind::KEYBOARD_HIDE),
        ("kHelloAck", kind::HELLO_ACK),
        ("kOperationResult", kind::OPERATION_RESULT),
        ("kSurface", kind::SURFACE),
        ("kFrameDone", kind::FRAME_DONE),
        ("kTouchDown", kind::TOUCH_DOWN),
        ("kTouchMove", kind::TOUCH_MOVE),
        ("kTouchUp", kind::TOUCH_UP),
        ("kKey", kind::KEY),
        ("kBack", kind::BACK),
        ("kKeyboardInset", kind::KEYBOARD_INSET),
    ] {
        assert_eq!(of(name), n, "{name}");
    }
    // The ranges the host accepts end where ours do.
    assert!(h.contains("? Kind::kKeyboardHide") && h.contains("? Kind::kKeyboardInset"));
}

/// The vendored v3 header is what the patch series produces.
#[test]
fn v3_header_is_the_patched_one() {
    let dir = env!("CARGO_MANIFEST_DIR");
    let vendored = std::fs::read_to_string(format!("{dir}/vendor/aera/protocol_v3.hpp")).unwrap();
    let v2 = header("protocol.hpp");
    // Rebuild the header from Host API 2 and the series' protocol.hpp hunks.
    let mut text = v2;
    let series = std::fs::read_to_string(format!("{dir}/third_party/aera/patches/series")).unwrap();
    for patch in series.lines() {
        let p = std::fs::read_to_string(format!("{dir}/third_party/aera/patches/{patch}")).unwrap();
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
    for (file, record) in [("vendor/flutter/flutter_embedder.h", &pin), ("vendor/aera/protocol.hpp", &host), ("vendor/aera/protocol_v3.hpp", &host)] {
        let bytes = std::fs::read(format!("{}/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let out = std::process::Command::new("sha256sum").arg(format!("{}/{file}", env!("CARGO_MANIFEST_DIR"))).output().unwrap();
        let hash = String::from_utf8(out.stdout).unwrap().split_whitespace().next().unwrap().to_owned();
        assert!(!bytes.is_empty());
        assert!(record.contains(&hash), "{file} is {hash}, which the spec does not record");
    }
}
