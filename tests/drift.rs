//! The drift gate: the numbers in `src/host/` match the vendored headers
//! (`vendor/aera/protocol.hpp` from AERA, `protocol_v3_assumed.hpp` ours).

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

#[test]
fn host_api_3_assumed_numbers() {
    let h = header("protocol_v3_assumed.hpp");
    assert_eq!(value(&h, "kProtocolVersion"), host::PROTOCOL_VERSION);
    assert_eq!(value(&h, "kSurfaceFd") as i32, host::SURFACE_FD);
    assert_eq!(value(&h, "kControlFd") as i32, host::CONTROL_FD);
    assert_eq!(value(&h, "kFeaturePixelSurface"), feature::PIXEL_SURFACE);
    assert_eq!(value(&h, "kFeatureKeyboardInset"), feature::KEYBOARD_INSET);
    for (name, n) in [
        ("kPresent", kind::PRESENT),
        ("kKeyboardShow", kind::KEYBOARD_SHOW),
        ("kKeyboardHide", kind::KEYBOARD_HIDE),
        ("kSurface", kind::SURFACE),
        ("kFrameDone", kind::FRAME_DONE),
        ("kTouchDown", kind::TOUCH_DOWN),
        ("kTouchMove", kind::TOUCH_MOVE),
        ("kTouchUp", kind::TOUCH_UP),
        ("kKey", kind::KEY),
        ("kBack", kind::BACK),
        ("kKeyboardInset", kind::KEYBOARD_INSET),
        ("kText", host::purpose::TEXT),
        ("kDigits", host::purpose::DIGITS),
    ] {
        assert_eq!(value(&h, name), n, "{name}");
    }
    // Every ASSUMED number in the header is marked.
    for line in h.lines().filter(|l| l.contains(" = ") && !l.contains("Host API 2")) {
        assert!(line.contains("ASSUMED"), "unmarked: {line}");
    }
}

#[test]
fn vendored_headers_are_the_recorded_ones() {
    let pin = std::fs::read_to_string(format!("{}/spec/engine-pin.md", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let host = std::fs::read_to_string(format!("{}/spec/host.md", env!("CARGO_MANIFEST_DIR"))).unwrap();
    for (file, record) in [("vendor/flutter/flutter_embedder.h", &pin), ("vendor/aera/protocol.hpp", &host)] {
        let bytes = std::fs::read(format!("{}/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let out = std::process::Command::new("sha256sum").arg(format!("{}/{file}", env!("CARGO_MANIFEST_DIR"))).output().unwrap();
        let hash = String::from_utf8(out.stdout).unwrap().split_whitespace().next().unwrap().to_owned();
        assert!(!bytes.is_empty());
        assert!(record.contains(&hash), "{file} is {hash}, which the spec does not record");
    }
}
