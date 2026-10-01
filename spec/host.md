# The AERA host contract

What flutter-aera relies on from AERA Recovery's generic pixel + GPU plugin
host. Host API 2 items are AERA's published contract; everything marked
**ASSUMED** is our guess at Host API 3, asked of AERA in
[aera-flutter-demo#1](https://github.com/1vivy/aera-flutter-demo/issues/1),
and changes the day AERA publishes.

This file, `vendor/aera/protocol_v3_assumed.hpp` and `src/host/` change
together. `tests/drift.rs` checks the numbers against both headers.

## Sources

| File | From | sha256 |
| --- | --- | --- |
| `vendor/aera/protocol.hpp` | AERA-Recovery/android_bootable_recovery@abf3316 `aeraui/features/plugin_api/protocol.hpp`, verbatim | `82d7b8df63bed0e68773931e955be119bce76ebda04efeeac03077a28ae56822` |
| `vendor/aera/protocol_v3_assumed.hpp` | ours, in AERA's style, every item ASSUMED | (ours; not pinned) |

Manifest and payload rules: `aeraui/features/plugins/plugin_manager.cpp`
at the same commit.

## Process

- Host API 2: AERA execs `usr/bin/aera-plugin` from the extracted payload,
  names the payload in `AERA_PLUGIN_ROOT` and the control socket in
  `AERA_PLUGIN_FD` (fd 4). Plugins run as root in recovery's namespaces;
  they are killed when their scene is left.
- **ASSUMED**: the same for Host API 3, launched with `--aera-host-api=3`.
- **ASSUMED**: `AERA_PLUGIN_DATA` names a persistent per-plugin directory.
- **ASSUMED**: the GPU (`/dev/kgsl-3d0`, the DMA heap) is free for the
  plugin while it is in front.

## Wire

Host API 2: `SOCK_SEQPACKET`, one 1144-byte `Message` per packet
(`magic` `A2PI`, `version`, `kind`, `request_id`, `value`, `flags`,
`title[96]`, `text[1024]`, little-endian, strings NUL-terminated). Packets
that fail `Valid()` are dropped.

**ASSUMED**: `version` is 3.

### Handshake

1. Plugin → `HELLO` (1), `value` = min, `flags` = max protocol version (3, 3).
2. Host → `HELLO_ACK` (64), `value` = features. **ASSUMED** bits:
   `PIXEL_SURFACE` (1 << 2), `KEYBOARD_INSET` (1 << 3). Host API 2 bits:
   `METRICS` (1 << 0), `BACK_NAVIGATION` (1 << 1). The embedder refuses a
   host without `PIXEL_SURFACE`.
3. Host → **ASSUMED** `SURFACE` (68): `value` width, `flags` height,
   `request_id` stride (bytes), `title` `BGRA8888`, `text`
   `slots=N scale=S refresh=HZ` (unknown keys ignored; defaults 3, 1.0, 60).

Messages other than these that arrive during the handshake are replayed
once the engine runs.

### Frames (ASSUMED)

- fd 3 (`AERA_SURFACE_FD`): a sealed memfd of `slots` frames, each
  `stride × height` bytes, BGRA8888 (B, G, R, A bytes), top-down.
- Plugin → `PRESENT` (12): `request_id` sequence (from 1, wraps, never 0),
  `value` slot.
- Host → `FRAME_DONE` (69): `request_id` sequence. The slot is the
  plugin's again. Sent when the frame has been shown (vsync), so the
  embedder paces on it: Flutter gets a vsync only while a slot is free.
- 3 slots: one on screen, one queued, one being drawn.

### Input (ASSUMED)

| Kind | Fields |
| --- | --- |
| `TOUCH_DOWN` 70, `TOUCH_MOVE` 71, `TOUCH_UP` 72 | `request_id` pointer id, `value` x, `flags` y, surface pixels |
| `KEY` 73 | `value` Unicode code point; 8 backspace, 13 Enter, also sent by the single-line keyboard's OK |
| `BACK` 74 | AERA's back gesture |
| `KEYBOARD_INSET` 75 | `value` keyboard height in surface pixels, 0 hidden; sent on every show, hide (including AERA's own) and resize |

Plugin → host: `KEYBOARD_SHOW` (13) with `value` purpose (0 text, 2
digits) and `flags` 1 for multiline; `KEYBOARD_HIDE` (14).

### Lifecycle

Host API 2: `LIFECYCLE` (66) `value` 1 resume, 2 pause, 3 stop; plugin
`CLOSE` (7) to leave. The embedder sends `CLOSE` when the app pops its
last route (`SystemNavigator.pop`) and exits on stop or when the socket
closes.

## Not used

Host API 2's declarative UI (`BEGIN_PAGE` … `SET_BACK_ACTION`) and
mediated operations: a Flutter app draws its own UI.

## Later (asked, not relied on)

dma-buf slots with sync_file fences (zero copy); damage rectangles on
`PRESENT`; a new `SURFACE` for resize/rotation; speaker audio through
`aera-audio-bridge` (not the embedder's: no Flutter system channel).
