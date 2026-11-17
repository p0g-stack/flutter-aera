# The AERA host contract

This branch uses upstream **Host API 2**, not the colliding Host API 3 wire
protocol. Base desktop rendering and input require no recovery patch. Optional
desktop keyboard control and Files-over-desktop require the small, negotiated
extension set in `third_party/aera/patches-api2/`.

## Sources and drift gate

Upstream revision: `1cabc50919d2587ba986f679f946f52963130fc1` in
[AERA-Recovery/android_bootable_recovery](https://github.com/AERA-Recovery/android_bootable_recovery/tree/1cabc50919d2587ba986f679f946f52963130fc1).

| Vendored file | Upstream source | SHA-256 |
| --- | --- | --- |
| `vendor/aera/protocol.hpp` | `aeraui/features/plugin_api/protocol.hpp` | `0443a091a7d2ce9cc71c0ea90aecf5a047e3cf53e0bd0157ca5d5f6085d6147f` |
| `vendor/aera/play_protocol.h` | `aeraui/components/play_protocol.h` | `bc8a59215572019ee21b2d73daa75c57b5551c8fe8c0becc5ab390111baf3346` |

`tests/drift.rs` checks the Rust wire numbers against the upstream headers and
the extended header reconstructed from the actual recovery patches. The old
`third_party/aera/patches/` series is historical comparison material on this
branch: **do not apply it together with the API 2 extensions**.

## Process and handshake

Upstream `features/plugin_api/launcher.cpp` passes only the control socket on
fd 4 (`AERA_PLUGIN_FD=4`), `AERA_HOST_API=2`, `--aera-host-api=2`, and
`AERA_PLUGIN_ROOT`. fd 3 is closed. The verified payload is expanded into
root-owned mode-0700 `/tmp/aera-p2-XXXXXX`. Plugins run as root in recovery's
namespaces. No persistent per-plugin data environment or appearance hint is
provided by upstream; see `aerap.md` for the resulting storage limitation.

Control packets are 1144 bytes: six little-endian u32 fields (`magic`
`0x41325049`, `version=2`, `kind`, `request_id`, `value`, `flags`), followed
by NUL-terminated `title[96]` and `text[1024]`. Transport is SOCK_SEQPACKET.
Upstream limits control requests to 128 per second; frames use a separate
channel and are not control packets.

1. Send `Hello=1`, minimum and maximum both 2, with
   `request_id=0xA2E20000 | requested_optional_bits`.
2. Read `HelloAck=64`, value 2, supported requested features in flags. The
   original metrics/back bits are also advertised. Require desktop bit 7;
   never send an optional request solely because its number is known.
3. `Lifecycle=66`, value 1, resumes the plugin.
4. Send `OpenDesktop=21`, nonzero request ID. Zero dimensions request
   960×540; with resolution bit 8 the allowed sizes are 640×360, 960×540,
   1280×720 and 1920×1080.
5. `DesktopReady=72` echoes the request ID and gives width/height in
   value/flags, with **exactly two SCM_RIGHTS descriptors**, frame memfd first
   and input SOCK_SEQPACKET second. Failure has zero dimensions and no fds.
   The receiver owns the delivered descriptors, validates packet truncation,
   dimensions and memfd size/seals, and closes invalid ancillary descriptors.

Messages received during setup must not be discarded. The descriptor-bearing
reply needs `recvmsg`, not `recv`.

## Frames, damage and timing

The shared memfd contains two tightly packed BGRA8888 frames, top-down,
`width * height * 4` bytes each. The host seals growth/shrinkage. Publish
sequence N into slot `N % 2`, starting at N=1. The desktop channel carries
20-byte `PlayMessage { u32 magic, kind, sequence; i32 x, y; }`, magic
`0x41575031`. Send `PLAY_FRAME=1` with x/y equal to dimensions.

Upstream's 16 ms desktop timer sends `PLAY_ACK=2` for its previous received
sequence, then receives and latches at most one new frame. It validates exact
sequence progression and dimensions. The embedder must preserve parity and
not reuse an unacknowledged slot; limiting publication to one outstanding
frame also avoids overwriting the displayed slot before its replacement.
ACK is **not** a compositor fence, scanout timestamp or display vsync. Flutter
uses estimated 60 Hz pacing with slot availability as backpressure.

There is no damage field: each submitted slot must be a complete image even
when Flutter repaints only part of its framebuffer. There is no in-place
resize/rotation event. The host opens a landscape overlay and restores the
setup orientation on EXIT. Scale 1 is an embedder policy, not host-reported
physical DPI. These restrictions also apply to the simulator's base mode.

## Desktop input

| Play kind | Meaning |
| --- | --- |
| KEY 3 | x X11 keysym, y key down/up |
| BUTTON 4 | x mouse button, y down/up |
| POINTER 5 | absolute desktop x/y |
| LOOK 6 | game-relative mouse motion, not multitouch |
| RELEASE 7 | release held input |
| CLOSE 8 | desktop closed |
| WHEEL 9 | upstream wheel amount in x |

MENU-mode touches become one pointer/button stream, not raw touch IDs. The
host's toolbar intercepts virtual x<580,y<65 (coordinates relative to a
960×540 layout); game mode sends game controls. ESC can be mapped to Flutter
`popRoute`, but is not proof that recovery's system edge-back is delivered to
Flutter. The base keyboard is user-opened with KEYS and only forwards ASCII
and selected special keysyms. Physical keys have separate down/up edges.

## Negotiated desktop integration extension

Feature bit 12 (`kFeatureDesktopIntegration`) adds:

- `SetDesktopKeyboard=29`: nonzero request ID, value 1 show / 0 hide;
  flags use input-number 2 or multiline 4, empty strings. The desktop keyboard
  sends `DesktopKeyboardInset=76`, value the bottom occlusion in desktop
  framebuffer pixels (zero hidden). Host toolbar changes also report it.
- Unicode keysyms from the desktop keyboard, retaining the existing
  `PLAY_KEY` packet. Unicode beyond Latin-1 uses `0x01000000 | scalar`.
- Existing `PickFile=28` may open over the desktop, suspending and releasing
  desktop input until the native overlay completes. Without bit 12 upstream
  explicitly rejects this request while a desktop is open.

`flutter/textinput` keeps Flutter's editing model and maps Play keys to
`TextInputClient.updateEditingState` / `performAction`; keyboard show/hide
uses kind 29 only when negotiated. Base hosts return not-implemented for
show/hide: the user can still open KEYS manually. No fake inset is supplied.

Feature bit 13 (`kFeatureDesktopTextInput`) separately permits native setup
fields to open modally over the desktop: `EditTextInput=30` and
`DismissTextInput=31`, request ID the existing field ID, all other fields
empty/zero. It reuses upstream `TextInputResult=74`. This native editor is
**not** a Flutter IME: it emits only submitted/cancelled results, has a
255-code-point limit, and sends no draft, selection or composing-range
updates. Flutter uses the incremental desktop keyboard path instead.

## Files

Feature bit 11 is upstream `kFeatureFilePicker`. `PickFile=28` has nonzero
request ID, value 0 file / 1 folder, flags zero, optional absolute starting
directory in text and comma-separated dotted extension filters in title
(for example `.zip,.img`; folder mode requires empty title).
`FilePickerResult=75` echoes the ID, value zero, with exactly one flag:
selected 1 (absolute path in text), cancelled 2 (empty text), error 4
(diagnostic in text). Only one picker is allowed.

The stock Linux `file_selector` Pigeon channel maps single file/directory
requests to this contract. Multi-select and save return explicit
`unsupported`, not an invented selection or a silently reduced request.
Overlong paths/filters are rejected rather than wire-truncated.

## Clipboard and lifecycle

Upstream clipboard feature bit 9 provides GET22/SET23/CHUNK24 and result73,
acknowledged chunks up to 1023 bytes, 1 MiB total. UTF-8 may span chunks.
It requires a foreground plugin view and no conflicting host modal. This
branch's Flutter platform handler still uses its existing process-local
clipboard; it does not claim native clipboard integration.

Lifecycle values remain resume 1, pause 2, stop 3. No API 3 inactive value
is sent. Desktop plugins cannot opt into upstream background mode. Leaving
the scene destroys the desktop and stops the process group; EXIT closes
the overlay and returns to setup. Channel loss and RELEASE/CLOSE must stop
input/rendering cleanly, not leave held buttons or a blocked raster thread.


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

## Remaining upstream gaps

See `docs/upstream-host-api-2.md` for feasibility evidence, the smallest
upstream-shaped additions, verification results, and Cuttlefish requirements.
No physical device is part of this branch's qualification.
