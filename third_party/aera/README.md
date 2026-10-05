# AERA Host API 3

A real Host API 3 for AERA Recovery, carried as a patch series against a
pinned AERA commit (the way the frb patches are carried) until it is offered
upstream.

pin: 92813466db89cc7450a708a4acc0010087028f8f

Rebased 2026-10-05 from abf3316 onto 9281346 (AERA's aera-16.0 head; image
aera-infiniti-arm64-a56b3ac-ca015146c92db600 is built from patches/ and
remote-patches/ at this pin). Two conflicts: 0005 with upstream's
theme-extension fonts (`IsThemeExtension`, `IsLaunchable`; both kept) and
0024 with upstream 58d3707, which accepts an edge swipe once it is captured
(`edge_contact_captured_`); the hold timer now runs on top of that capture.
F0001 kept its content and now leaves upstream's capture in the pointer
read; C0001 applied unchanged.

Upstream: <https://github.com/AERA-Recovery/android_bootable_recovery>
(`bootable/recovery` in AERA's manifest). The patches touch `aeraui/`, `aera_rpc/`
(0008) `prebuilt/` (0010, 0014), `minuitwrp/` (0013, 0025) and the
top-level `Android.mk` (0014); 0026 touches `aeraui/core/engine.cpp`. 0017,
0027 and 0023 moved to `remote-patches/` (R0001, R0002, R0003), 0028 to
`refinement-patches/` (F0001) and 0011 to `cuttlefish-patches/` (C0001);
0012 was dropped (Yuv, patch audit 2026-10-03: log spam only, never seen
on hardware). Their numbers stay unused so earlier references keep their
meaning.

Apply with `git am $(cat patches/series)` on a checkout of the pin, then
`cuttlefish-patches/` (x86_64 builds only), `refinement-patches/` and
`remote-patches/` (below), in that order. Devicelab copies the `*.patch`
files of each to `aera/build/patches/bootable/recovery/`; the `C`, `F` and
`R` names sort after the numbered ones in that order. Each extra series
applies on `patches/` alone.

| Patch | What |
| --- | --- |
| 0001 | wire: version 3, kinds 12–14 and 68–75, features, surface constants |
| 0002 | session: per-scene version, `SURFACE` before `RESUME`, `PRESENT` outside the rate limit |
| 0003 | `plugin_api/surface.*`: sealed memfd slots and who owns each |
| 0004 | launcher: surface on fd 3, `AERA_SURFACE_FD`, `AERA_PLUGIN_DATA`, `--aera-host-api=3` |
| 0005 | plugin manager: v3 manifests (`pixel-surface` …), data directory, removed with the plugin |
| 0006 | `scenes/pixel_plugin_scene.cpp`, routing, the engine's pointer hook |
| 0007 | tests and the Host API 3 section of `plugin_api/README.md` |
| 0008 | AERA RPC `plugin` / `open`: start an installed plugin over adb, for `flutter attach` (docs/debugging.md) |
| 0009 | `AERA_APPEARANCE=light\|dark` in a Host API 3 plugin's environment, from AERA's theme (platform brightness) |
| 0010 | `prebuilt/Android.mk`: pack `libincfs.so` on every build (`libandroidfw.so` needs it), not only with FBE |
| 0013 | `minuitwrp/graphics_drm.cpp`: without a Qualcomm SDE topology, scan out on the CRTC's own primary plane with one layer mixer (devicelab D3) |
| 0014 | `task_profiles.json` required and packed on every build, so logd stops aborting on builds that ship logd (`TARGET_USES_LOGD := true`) without the file in their device tree (devicelab D4) |
| 0015 | pixel plugin scene: a contact in the side-edge zone is Back only once it swipes inward; taps there reach the plugin (devicelab D5) |
| 0016 | `aeraui/core/engine.cpp`: take an RPC request whose writer already hung up (POLLHUP), so a request is never left waiting for the next client (devicelab D6) |
| 0018 | `aeraui/components/file_picker`: a shared file picker (one file, several, a folder, save as) that browses like Files; Telegram's attach picker becomes a caller, confined to its roots as before |
| 0019 | `plugin_api`, pixel plugin scene: `REQUEST_OPERATION` `kPickFiles` opens that picker for a pixel plugin (`kFeatureFilePicker`); paths come back as `OPERATION_RESULT`s |
| 0020 | `plugin_api`, pixel plugin scene, engine: a rotation sends a new `SURFACE` on the same memfd (sized at launch for either orientation) instead of restarting the plugin; `PRESENT` `flags` carry the surface generation and stale frames are released; the Quick Settings shade the rotation came from is laid out again for the new size |
| 0021 | `plugin_api`, pixel plugin scene, engine: leaving the scene pauses the plugin (no `FRAME_DONE`) instead of stopping it; reopening sends a new `SURFACE` and resume; `kInactive` for a shade, sheet or picker over it; stopped by `CLOSE`, Recents or an update |
| 0022 | `plugin_api`: `AERA_PLUGIN_DATA_VOLATILE=1` when the data directory is the RAM fallback |
| 0024 | `core/runner.cpp`, engine: a Back held for half a second (key or edge swipe) skips the pixel plugin and leaves its scene, so a plugin can never trap the user (Yuv's call) |
| 0025 | `minuitwrp/events.cpp`: a new touch contact starts at its slot's last position, so a second tap at the same x (or the same spot) is no longer reported at x=0 or dropped, on whichever slot the kernel is on (devicelab D9; the Infiniti panel's raw path was not on slot 0) |
| 0026 | `aeraui/core/engine.cpp`: the software renderer (no Adreno) draws landscape into a landscape-shaped buffer and turns each frame upright on flush, instead of folding a landscape layout into the portrait scanout (devicelab D12) |
| 0029 | `aeraui/scenes/pixel_plugin_scene.cpp`: the surface is scaled to the panel on each axis, so with adaptive resolution it is exactly the panel's width (it was 1279 on a 1272 panel; Infiniti hand walk) |

`cuttlefish-patches/` is the series only an emulator build needs: nothing
in it matters on an arm64 phone (harmless there).

| Patch | What |
| --- | --- |
| C0001 | libwebp's SSE2/SSE4.1 sources, so recovery links for x86_64 (Cuttlefish; was 0011) |

`refinement-patches/` is the series for refinements of AERA's own behaviour:
optional polish that AERA works without and that a plugin does not need,
kept apart from the defect fixes above (Yuv, 2026-10-02). Applied after
`patches/`; it needs nothing from `remote-patches/`.

| Patch | What |
| --- | --- |
| F0001 | `aeraui/core/engine.cpp`: a contact in the bottom strip starts the Recents swipe only once it moves up past the touch slop; a tap or a sideways contact goes to what is under it (pixel plugin, browser or AERA's own UI) instead of flashing the Recents preview and being lost. Optional polish (Infiniti hardware walk; was 0028) |

`remote-patches/` is the series for AERA Remote (`aera_remote/`: its server,
input injection and screen capture), against the same pin and applied after
`patches/` and `refinement-patches/` (`git am $(cat remote-patches/series)`).
It needs nothing from `refinement-patches/` today. AERA Remote work goes here (Yuv, 2026-10-02).

| Patch | What |
| --- | --- |
| R0001 | `aera_remote/input.cpp`, `aera_remote.cpp`: create the virtual input device when Remote starts, so the first touch's press is not lost before recovery's input reader opens it (devicelab D7; was 0017) |
| R0002 | `aera_remote/aera_remote.cpp`: `/screen.jpg` waits (up to 0.5 s) for a frame captured after the request, instead of returning the one from the previous request (devicelab D12; was 0027) |
| R0003 | `core/runner.cpp`, engine: the Home and Menu keys (AERA Remote's buttons, KEY_HOMEPAGE and KEY_MENU) show Home and toggle Recents, as the bottom-edge swipe does (devicelab D8; was 0023, on top of 0024 since the 2026-10-03 audit) |

`lvgl-patches/` is a separate series for AERA's LVGL fork
(`external/lvgl`, android_external_lvgl, pinned at
212fd3a25187f9876356ae66aa13103b002bc322 in AERA's manifest). Apply with
`git am $(cat lvgl-patches/series)` there (0001, a blend-clip backstop nothing reached once 0002 was in, was dropped in the 2026-10-03 audit); devicelab copies them to
`aera/build/patches/external/lvgl/`.

| Patch | What |
| --- | --- |
| 0002 | `lv_conf.h`: `LV_DRAW_TRANSFORM_USE_MATRIX 0`. Neither of AERA's renderers applies a draw task's matrix, so transforms were never drawn and the widened clip let shrunk, off-screen objects blend outside the display buffer (the D10/D12 crash). Transforms now go through layers and scale as the styles ask (pressed buttons, tiles and cards shrink slightly; enlarged icons are drawn enlarged) |
| 0003 | `src/draw/lv_draw.c`: a layer larger than the whole `LV_DRAW_LAYER_MAX_MEMORY` budget is allocated when no other layer holds memory, instead of being left for later forever. With 0002 transforms use layers, and on an adaptive-resolution screen (logical 1440x2696) a Recents card shrunk on press needs more than AERA's 4 MiB: tapping it hung recovery (devicelab D13) |

What differs from our first guess (aera-flutter-demo#1):
`HELLO_ACK` carries the version in `value` and the features in `flags`, as
Host API 2 already does, and a slot comes back with `FRAME_DONE` only once a
newer frame has replaced it on screen (or at once if a newer `PRESENT`
superseded it before it was shown). The embedder and the sim follow both.

Checked by `ci/aera-host-check.sh` (session and surface unit tests, then the
counter through AERA's launcher, session and surface). The LVGL scene is not
run in CI: it builds only inside the AERA tree.

Not yet: dma-buf slots, damage rectangles. Sound needs no patch
(`spec/host.md`, Sound). Every known AERA gap, fixed or worked
around, is in `docs/aera-deficiencies.md`.

Licence: the patches are AERA's code and stay under its Apache-2.0.
