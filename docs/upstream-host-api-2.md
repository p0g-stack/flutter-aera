# Upstream Host API 2 feasibility spike

## Verdict

**Upstream API 2 can carry Flutter frames and desktop pointer/key input. It
cannot supply the complete requested Flutter keyboard-and-picker contract
without recovery changes.** This branch implements the base path and a small,
feature-negotiated extension set instead of renumbering Host API 3.

The decisive blockers are in the host's scene policy, not in Rust transport:

- [`generic_plugin_scene.cpp:765–768`](https://github.com/AERA-Recovery/android_bootable_recovery/blob/1cabc50919d2587ba986f679f946f52963130fc1/aeraui/scenes/generic_plugin_scene.cpp#L765-L768)
  explicitly requires `!desktop_open` for native Files selection.
- [`generic_plugin_scene.cpp:358–363`](https://github.com/AERA-Recovery/android_bootable_recovery/blob/1cabc50919d2587ba986f679f946f52963130fc1/aeraui/scenes/generic_plugin_scene.cpp#L358-L363)
  prevents opening a native editor while a desktop is open. Native field cards
  only open by a trusted UI click; AddTextInput does not focus a Flutter field.
- [`plugin_desktop.cpp:292–319`](https://github.com/AERA-Recovery/android_bootable_recovery/blob/1cabc50919d2587ba986f679f946f52963130fc1/aeraui/components/plugin_desktop.cpp#L292-L319)
  creates a user-opened keyboard, emits only ASCII plus selected keysyms,
  and reports no visibility or geometry.
- Closing the input socket is **not** a workaround for the picker guard:
  [`plugin_desktop.cpp:327–334`](https://github.com/AERA-Recovery/android_bootable_recovery/blob/1cabc50919d2587ba986f679f946f52963130fc1/aeraui/components/plugin_desktop.cpp#L327-L334)
  displays “Application stopped. Use EXIT to return to setup.”; it does not
  clear `desktop_open` or return to the setup scene.

A simulator that let PickFile or AddTextInput open over an unpatched desktop
would prove a different protocol. `--base-api2` must preserve these refusals.

## Contract evidence and gaps

All source links below are fixed to recovery
`1cabc50919d2587ba986f679f946f52963130fc1` (not mutable upstream HEAD).
The source root is
[`aeraui/`](https://github.com/AERA-Recovery/android_bootable_recovery/tree/1cabc50919d2587ba986f679f946f52963130fc1/aeraui).

| Capability | Upstream fact | Smallest upstream-shaped addition / branch policy |
| --- | --- | --- |
| Launch and handshake | `features/plugin_api/launcher.cpp:71–104` closes fd 3, passes fd 4 and API 2 argv/env. `session.cpp:115–133` acknowledges `0xA2E20000 | bits`. | No recovery change. Receive descriptors with recvmsg, not inherited fd 3. |
| Frame surface | `session.cpp:67–90`: DesktopReady72 passes exactly two SCM_RIGHTS fds. `components/plugin_desktop.cpp:75–86` allocates size-sealed two-slot memfd and input seqpacket. | No recovery change. Validate fd count, sizes/seals, negotiated dimensions. |
| Publication | `components/play_protocol.h`: 20-byte PlayMessage; FRAME1 and ACK2. `plugin_desktop.cpp:320–339` checks dimensions and exact next sequence; slot=N%2. | No recovery change. One outstanding sequence, complete BGRA frames; 16 ms ACK-paced host loop. |
| Damage | No rectangle, stride or format negotiation in PlayMessage. | Copy a complete frame to each slot. A later optional Play damage packet would need explicit host negotiation; not needed for correctness here. |
| Vsync/frame-done | ACK is emitted on a 16 ms LVGL timer before receiving the next frame. No timestamp, scanout fence or refresh-rate report. | Estimated 60 Hz pacing is sufficient for the spike, not a real-vsync claim. A negotiated timestamped presentation event would be the smallest timing addition. |
| Touch/mouse | MENU maps contacts to POINTER5/BUTTON4; no contact IDs. Top toolbar consumes virtual x<580,y<65. PLAY mode is a game control layout. | Single-pointer Flutter interaction, not raw multitouch. A negotiated direct-touch mode would need contact ID/phase plus a content rectangle excluding host controls. |
| Back | Desktop toolbar ESC sends X11 Escape; EXIT closes desktop. Declarative SetBackAction targets setup-page buttons. | Map Escape to Flutter popRoute. Recovery edge/system Back equivalence is not established; a negotiated desktop Back event is needed to guarantee it without confusing EXIT. |
| Resize/rotation/DPI | `core/engine.cpp:2591–2604` forces landscape and restores orientation; allowed sizes are four fixed 16:9 resolutions. No reshape event or DPI value. | Fixed negotiated desktop size and scale 1. Live rotation would require a new surface-generation/resize handshake with replacement descriptors, not reused API 3 numbers. |
| Desktop keyboard | No request to show/hide or geometry notification; ASCII/special keys only. | Feature bit12, SetDesktopKeyboard29, DesktopKeyboardInset76, Unicode keysyms on the existing Play channel. |
| Native text editor | AddTextInput25 is a setup-page card; max255 code points; only submitted/cancelled/error result74. No draft/selection/composition messages. | Feature bit13, EditTextInput30 and DismissTextInput31 open/cancel the existing native field over a suspended desktop. This remains a modal field editor, not a Flutter IME. Flutter uses incremental Play keys. |
| File picker | PickFile28/result75 exists, but desktop_open is explicitly denied. File0/folder1 only; title contains dotted extension filters and text the start directory. | Bit12 allows the existing picker over a suspended desktop. No new picker wire kinds. |
| Multi-file/save | Native Files-selection mode disables create/multiselect, accepts one existing file or directory. | Standard file_selector returns explicit unsupported. A later optional picker-modes bit could add modes and an explicit bounded multi-result sequence; no silent single-file/save approximation. |
| Clipboard | Bit9 uses GET22, SET23, CHUNK24, result73, offset-acknowledged UTF-8 byte chunks up to1MiB, foreground-only. | Already upstream. This branch retains the pre-existing process-local Flutter clipboard and does not claim shared clipboard integration. |
| Lifecycle/background | Resume1/pause2/stop3. Desktop open rejects background workers; SetBackground rejects an open desktop. Destruction sends stop and kills process group. | Preserve upstream semantics. Persistent Recents would require a distinct negotiated suspend/resume surface contract; API3 inactive4 is not reused. |
| Persistent paths/theme | Upstream launcher has no AERA_PLUGIN_DATA / VOLATILE / APPEARANCE. | Do not claim old API3 persistence. Optional local env override remains; selected paths are app-managed. |
| Manifest | `features/plugins/plugin_manager.cpp:327–355`: schema1, ui-runtime/main, API/protocol2, usr/bin/aera-plugin; desktop-surface accepted, old pixel/GPU/network/audio permissions rejected. | `spec/aerap.md` describes API2. No consumer packer/pin is changed by this branch. |

## Extension design

`third_party/aera/patches-api2/series` is the sole recovery protocol patch set
for this branch. New feature bits follow upstream bit11, worker kinds follow
28 and host kinds follow75. Old values and unmarked HELLO behavior are retained.
The patches carry PR-ready rationale and test instructions in that directory.
They are preparation branches only: no upstream PR is opened.

The modal additions must release held desktop input, suppress input beneath
Files/native text overlays, and restore input on submit, cancellation, page
replacement and destruction. Keyboard geometry is computed in negotiated
framebuffer coordinates, not guessed from the Flutter window. The base mode
must never advertise or emit an extension it did not negotiate.

## Patch size and application proof

The new set contains **2 patches**, touching **8 distinct recovery files**,
with **449 added / 45 removed lines** and 47,542 bytes of mail patches.
Counts include implementation, tests and protocol documentation. For comparison:

| Series | Patches | Distinct files | Added / removed lines | Patch bytes |
| --- | ---: | ---: | ---: | ---: |
| New API 2 desktop extensions | 2 | 8 | 449 / 45 | 47,542 |
| Old Host API 3 + shared picker subset | 16 | 31 | 3,011 / 451 | 227,265 |
| Entire old recovery series | 23 | 35 | 3,255 / 468 | 259,292 |

The protocol/picker comparison excludes unrelated 0010, 0013, 0014, 0016,
0025, 0026 and 0030. Counts are sums from `git apply --numstat`, so a line
changed twice across a series is counted twice, not a net tree diff.

Both patches applied in order with `git am --3way` to a separate ovnfs
checkout detached at `1cabc509`, without conflicts. The applied
`plugin_api_session_test` compiled with `-std=c++17 -UNDEBUG -Wall -Wextra
-Werror` and passed. `plugin_desktop.cpp` and `plugin_runtime_ui_test.cpp`
(which includes the production generic scene) passed `-fsyntax-only` using
the configured recovery LVGL headers. An initial attempt with `LV_CONF_SKIP`
failed because the required Montserrat fonts were disabled; using the actual
`lv_conf.h` fixed the test invocation, not the production source. The full
LVGL runtime UI test executable and recovery image have not been run here.


## Executed host proof (2026-11-17)

Environment: Flutter **3.47.5 / Dart3.13.4**, unchanged
`engine-af7e796-0d7f3ed2` x64 debug artifact (download SHA-256 verified),
960×540 desktop. The host GL renderer reported NVIDIA GeForce RTX5090;
these are host-computer captures, not recovery GPU qualification.
The probe uses `file_selector`1.0.4, resolving `file_selector_linux`0.9.4+1.

- `cargo test --workspace`: **105 passed** (77 embedder,21 simulator,
  2 launcher,5 header/patch drift), zero failures.
- `cargo build --workspace`: passed. There are non-fatal unused simulator
  getter/import warnings; no linter or formatter was run.
- Base API2: features `0x983` (no bit12/13), first frame864ms,49 frames;
  two real taps logged count1 then count2, before/after PNGs differ, Details
  followed by Escape logged `api2: back returned`, child exit0.
- Extended API2: features `0x1983`, first frame992ms,53 frames; inset
  `0 → 297 → 0`, real TextField submission `héllo 😀`, native PickFile
  request1 selected the fixture path and the stock file_selector future
  returned it, Details/Back returned, child exit0.
- After correcting the simulator's extension Done-key behavior to match
  patch1, its focused Unicode/confirmation test passed (1/1), followed by
  the successful extended run above. The initial script used incorrect
  guessed hit coordinates; captures identified the real field/picker/Details
  positions. Those coordinates are now in `ci/api2-check.sh`.
- A shutdown race discovered by the first run was fixed: a platform task
  requesting Exit no longer falls through to another poll/read and turns
  the host's expected EOF into exit1. The successful runs above exited0.

Evidence root: `/tmp/aera-api2-proof.50c1P0/run.WQUJLh/`.

| Evidence | Path below that root |
| --- | --- |
| Counter before | `base/before.png` |
| Counter after (Count:2) | `base/after.png` |
| Base input and lifecycle log | `base.log` |
| Focused field / inset | `extended-final.aQG0Ly/focused.png` |
| UTF-8 typed into Flutter | `extended-final.aQG0Ly/typed.png` |
| Submitted text, keyboard hidden | `extended-final.aQG0Ly/submitted.png` |
| Picker returned path | `extended-final.aQG0Ly/picked.png` |
| Returned route | `extended-final.aQG0Ly/returned.png` |
| Extended wire/app log | `extended-final.aQG0Ly/run.log` |

The simulator assumes desktop-surface permission and a scale1 panel; picker
and native-editor fixtures represent a simulated user's answer after300ms.
It does not advertise background/activity/rich-presentation extensions and
fails on unmodelled requests rather than silently accepting them. Clipboard
chunks end on UTF-8 boundaries, which is wire-valid but not upstream's exact
1023-byte splitting. Teardown logs a best-effort status-send Broken pipe
after the host closes; both successful children exit0. The native modal
editor is covered by protocol/model tests, **not** a recovery UI capture.

## Qualification boundary

The host simulator checks actual Rust embedder/Flutter execution and wire
behavior, not recovery's LVGL compositor. It does not establish Cuttlefish GPU
correctness or phone compatibility. The API3 suite's old captures do not count
as API2 evidence. No physical device is selected, unlocked, written or flashed.

A Cuttlefish qualification image needs:

1. Recovery at the exact upstream revision above (or a separately recorded new
   upstream HEAD), a compatible manifest including libsmb2/libssh2 additions,
   and audited pins for every newly introduced upstream project/module.
2. This branch's API2 extension patches, **not** the old Host API3 series.
3. The separately rebased non-protocol series: libincfs (0010), primary-plane
   fallback (0013), task_profiles (0014), RPC POLLHUP (0016), the remaining MT
   slot fixes (0025), software landscape (0026), layer clipping (0030), remote
   input startup/fresh capture (R0001/R0002), and remaining Menu routing
   (R0003). Actual applicability comes from the separate rebase track, not
   the old textual survey. Drop merged F0001; do not carry the old shared
   picker0018 or pixel-host0019 in place of upstream Files.
4. Cuttlefish-specific libwebp SSE fix C0001 and emulator-only adbd init patch.
   The original LVGL optional patches must be evaluated against the chosen
   LVGL pin rather than assumed necessary or already applied.
5. Debug x64/arm64 runtime assets built from the intended Flutter/engine pin,
   an API2 manifest, and simulated recovery installation/launch. Current
   consumer packers and kit pins remain unchanged pending qualification.
6. Capture-and-input checks for first frame, tap/Back, UTF-8 text/inset,
   Files selection/cancel/error, modal input isolation, EXIT/orientation
   restoration, socket teardown and repeated launch. Qualify unpatched base
   API2 separately from the negotiated extensions. Explicitly exercise host
   keyboard controls and physical-key emulation in Cuttlefish, not only the
   simulator's scripted packets.

No release/AOT, live rotation, real-vsync, raw multitouch, persistent Recents,
shared clipboard or durable app-data parity is implied by this spike.
