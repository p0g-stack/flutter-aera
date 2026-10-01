# The AERA host contract

What flutter-aera relies on from AERA Recovery's pixel plugin host. Host
API 2 is AERA's published contract. Host API 3 is implemented by our patch
series for AERA in [`third_party/aera/`](../third_party/aera/README.md),
against AERA abf3316, and is offered upstream from there; until AERA takes
it, this file is its contract.

This file, `vendor/aera/protocol_v3.hpp`, the patches and `src/host/`
change together. `tests/drift.rs` checks our numbers against both headers
and that `protocol_v3.hpp` is what the patches produce.

## Sources

| File | From | sha256 |
| --- | --- | --- |
| `vendor/aera/protocol.hpp` | AERA-Recovery/android_bootable_recovery@abf3316 `aeraui/features/plugin_api/protocol.hpp`, verbatim | `82d7b8df63bed0e68773931e955be119bce76ebda04efeeac03077a28ae56822` |
| `vendor/aera/protocol_v3.hpp` | the same file after `third_party/aera/patches` | `d3b95b320e8626d92b2805cc7c397819f03d65519261f0488cd614a3b2c6c73f` |

Manifest and payload rules: `aeraui/features/plugins/plugin_manager.cpp`
(patch 0005); see `spec/aerap.md`.

## Process

- Host API 2: AERA execs `usr/bin/aera-plugin` from the payload expanded in
  RAM (`/tmp/aera-p2-XXXXXX`, root-owned, 0700) with a fixed environment:
  `AERA_PLUGIN_ROOT`, `AERA_PLUGIN_FD=4` (the control socket), `PATH`,
  `HOME=/tmp`, `LANG`, `AERA_LOCALE`. Plugins run as root in recovery's
  namespaces and are killed with their process group when their scene is
  left.
- Host API 3 adds `--aera-host-api=3`, `AERA_HOST_API=3`,
  `AERA_SURFACE_FD=3` and `AERA_PLUGIN_DATA`, a private directory that
  survives updates (`/sdcard/AERA/plugin-data/<id>`, or RAM when storage is
  not mounted), and `AERA_APPEARANCE=light|dark` from AERA's theme.
- Nothing gates the GPU (`/dev/kgsl-3d0`, the DMA heap): plugins are root.

## Wire

Host API 2: `SOCK_SEQPACKET`, one 1144-byte `Message` per packet
(`magic` `A2PI`, `version`, `kind`, `request_id`, `value`, `flags`,
`title[96]`, `text[1024]`, little-endian, strings NUL-terminated). A packet
that fails `Valid()` ends the session. At most 128 plugin messages a
second, `PRESENT` excepted.

Host API 3 packets carry `version` 3.

### Handshake

1. Plugin → `HELLO` (1), `value` = min, `flags` = max protocol version
   (3, 3). A pixel scene accepts only a range that includes 3.
2. Host → `HELLO_ACK` (64), `value` = 3, `flags` = features:
   `BACK_NAVIGATION` (1 << 1), `PIXEL_SURFACE` (1 << 2), `KEYBOARD_INSET`
   (1 << 3), `FILE_PICKER` (1 << 4). The embedder refuses a host without
   `PIXEL_SURFACE`.
3. Host → `SURFACE` (68): `value` width, `flags` height, `request_id`
   stride (bytes), `title` `BGRA8888`, `text` `slots=N scale=S refresh=HZ`
   (unknown keys ignored; defaults 3, 1.0, 60). AERA sizes it to the panel
   pixels below its status bar, `scale` from `ro.sf.lcd_density` / 160.
4. Host → `LIFECYCLE` (66) `RESUME`.

Messages other than these that arrive during the handshake are replayed
once the engine runs.

### Frames

- fd 3: a sealed memfd of `slots` frames, each `stride × height` bytes,
  BGRA8888 (B, G, R, A bytes), top-down.
- Plugin → `PRESENT` (12): `request_id` sequence (from 1, wraps, never 0
  or repeated), `value` a slot the plugin owns.
- Host → `FRAME_DONE` (69): `request_id` sequence; that slot is the
  plugin's again. AERA sends it at once for a frame a newer `PRESENT`
  superseded before it was shown, and otherwise when a newer frame has
  replaced it on screen: the frame on screen stays AERA's, since LVGL may
  redraw from it. With 3 slots one is on screen, one waits for the next
  refresh, one is being drawn. The embedder grants Flutter a vsync only
  while a slot is free.
- Rotation (patch 0020): the memfd is sized at launch for either
  orientation (`slots × long side² × 4` bytes). When AERA rotates, it sends
  a new `SURFACE` on the same fd with the new width, height and stride;
  that starts a new generation (0 for the first), and every slot is the
  plugin's again. `PRESENT` `flags` carries the generation the frame was
  drawn for; AERA answers a frame of an older generation with `FRAME_DONE`
  and does not show it. AERA's rotation toggle no longer leaves a running
  pixel plugin.

### Input

| Kind | Fields |
| --- | --- |
| `TOUCH_DOWN` 70, `TOUCH_MOVE` 71, `TOUCH_UP` 72 | `request_id` pointer id, `value` x, `flags` y, surface pixels |
| `KEY` 73 | `value` Unicode code point; 8 backspace, 13 Enter, also sent by the single-line keyboard's OK |
| `BACK` 74 | AERA's Back (gesture or key) |
| `KEYBOARD_INSET` 75 | `value` keyboard height over the surface in pixels, 0 hidden; sent on every show, hide (including AERA's own) and resize |

AERA keeps touches that start on the side edges (its Back gesture) or the
bottom edge (Recents), and those over its keyboard.

Plugin → host: `KEYBOARD_SHOW` (13) with `value` purpose (0 text, 2
digits) and `flags` 1 for multiline; `KEYBOARD_HIDE` (14).

### Lifecycle

Host API 2: `LIFECYCLE` (66) `value` 1 resume, 2 pause, 3 stop; plugin
`CLOSE` (7) to leave, after which AERA navigates back. The embedder sends
`CLOSE` when the app pops its last route (`SystemNavigator.pop`) and exits
on stop or when the socket closes.

### Files

`FILE_PICKER` (patch 0019): plugin → `REQUEST_OPERATION` (6) with a
non-zero `request_id` and `value` 8 (`kPickFiles`) opens AERA's file picker
(patch 0018, the same browsing as AERA's Files) over the surface. `flags`
is the mode (0 one file, 1 several, 2 a folder, 3 a name to save as),
`title` the folder to start in (empty: the current storage), `text` the
extensions to show (`zip,img`; empty: all) or, to save, the suggested name.
Host → `OPERATION_RESULT` (67) per chosen path: `value` 1, `text` the
absolute path, `flags` 1 while more follow. A closed picker answers `value`
0 with empty `text`; a refusal (one picker at a time) `value` 0 with the
reason. No permission or prompt: the pick is the user's consent. The
embedder answers the stock `file_selector` plugin's Linux channel with it.

## Sound

Not a Host API message: AERA's own features (Browser, Media, Streams, Doom,
RetroArch) start `/system/bin/aera-audio-bridge --browser-audio` themselves
when it exists and stop it when they leave (SIGTERM, then SIGKILL after
half a second), and so does the embedder (`src/audio.rs`). The bridge is a
closed binary only official AERA builds ship; without it there is no sound.
It listens on the abstract stream socket `aera-browser-audio-v1`; a client
checks the peer is root, sends four little-endian u32s (`0x41525041`
"APRA", 48000, 2, 16) and then interleaved S16LE 48 kHz stereo until it
closes. Read from `aeraui/features/browser/gst_aera_audio_sink.c` and the
features' `launcher.cpp`; AERA's volume (`/tmp/aera-audio-volume`) is
AERA's. In the payload, ALSA's default device writes there
(`audio/pcm_aera.c`).

## Not used

Host API 2's declarative UI (`BEGIN_PAGE` … `SET_BACK_ACTION`), which ends
a pixel session, and the other mediated operations, which AERA refuses to
pixel plugins: a Flutter app draws its own UI.

## Later (not in the patches yet)

dma-buf slots with sync_file fences (zero copy); damage rectangles on
`PRESENT`.
