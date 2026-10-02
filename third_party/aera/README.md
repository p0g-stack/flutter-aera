# AERA Host API 3

A real Host API 3 for AERA Recovery, carried as a patch series against a
pinned AERA commit (the way the frb patches are carried) until it is offered
upstream.

pin: abf33169b27dee17123c3c436a7299427821b329

Upstream: <https://github.com/AERA-Recovery/android_bootable_recovery>
(`bootable/recovery` in AERA's manifest). The patches touch `aeraui/`, `aera_rpc/`
(0008) `prebuilt/` (0010, 0014), `recovery_utils/` (0012), `minuitwrp/`
(0013, 0025), `aera_remote/` (0017, 0027) and the top-level
`Android.mk` (0014); 0026 touches `aeraui/core/engine.cpp`.

Apply with `git am $(cat patches/series)` on a checkout of the pin. Devicelab
copies `patches/*.patch` to `aera/build/patches/bootable/recovery/` for the
Cuttlefish image.

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
| 0011 | libwebp's SSE2/SSE4.1 sources, so recovery links for x86_64 (Cuttlefish) |
| 0012 | `recovery_utils/battery_utils.cpp`: look the health HAL up once (again at most once a minute if missing, or after it dies) instead of on every 1 s battery read (devicelab D2) |
| 0013 | `minuitwrp/graphics_drm.cpp`: without a Qualcomm SDE topology, scan out on the CRTC's own primary plane with one layer mixer (devicelab D3) |
| 0014 | `task_profiles.json` required and packed on every build, so logd stops aborting on builds that ship logd (`TARGET_USES_LOGD := true`) without the file in their device tree (devicelab D4) |
| 0015 | pixel plugin scene: a contact in the side-edge zone is Back only once it swipes inward; taps there reach the plugin (devicelab D5) |
| 0016 | `aeraui/core/engine.cpp`: take an RPC request whose writer already hung up (POLLHUP), so a request is never left waiting for the next client (devicelab D6) |
| 0017 | `aera_remote/input.cpp`, `aera_remote.cpp`: create the virtual input device when Remote starts, so the first touch's press is not lost before recovery's input reader opens it (devicelab D7) |
| 0018 | `aeraui/components/file_picker`: a shared file picker (one file, several, a folder, save as) that browses like Files; Telegram's attach picker becomes a caller, confined to its roots as before |
| 0019 | `plugin_api`, pixel plugin scene: `REQUEST_OPERATION` `kPickFiles` opens that picker for a pixel plugin (`kFeatureFilePicker`); paths come back as `OPERATION_RESULT`s |
| 0020 | `plugin_api`, pixel plugin scene, engine: a rotation sends a new `SURFACE` on the same memfd (sized at launch for either orientation) instead of restarting the plugin; `PRESENT` `flags` carry the surface generation and stale frames are released |
| 0021 | `plugin_api`, pixel plugin scene, engine: leaving the scene pauses the plugin (no `FRAME_DONE`) instead of stopping it; reopening sends a new `SURFACE` and resume; `kInactive` for a shade, sheet or picker over it; stopped by `CLOSE`, Recents or an update |
| 0022 | `plugin_api`: `AERA_PLUGIN_DATA_VOLATILE=1` when the data directory is the RAM fallback |
| 0023 | `core/runner.cpp`, engine: the Home and Menu keys (AERA Remote's buttons) show Home and toggle Recents, as the bottom-edge swipe does (devicelab D8) |
| 0024 | `core/runner.cpp`, engine: a Back held for half a second (key or edge swipe) skips the pixel plugin and leaves its scene, so a plugin can never trap the user (Yuv's call) |
| 0025 | `minuitwrp/events.cpp`: a new touch contact starts at its slot's last position, so a second tap at the same x (or the same spot) is no longer reported at x=0 or dropped (devicelab D9) |
| 0026 | `aeraui/core/engine.cpp`: the software renderer (no Adreno) draws landscape into a landscape-shaped buffer and turns each frame upright on flush, instead of folding a landscape layout into the portrait scanout (devicelab D12) |
| 0027 | `aera_remote/aera_remote.cpp`: `/screen.jpg` waits (up to 0.5 s) for a frame captured after the request, instead of returning the one from the previous request (devicelab D12) |

`lvgl-patches/` is a second, separate series for AERA's LVGL fork
(`external/lvgl`, android_external_lvgl, pinned at
017abcbf759c20ee9b91e0bf22e6ee81e04598a1 in AERA's manifest). Apply with
`git am $(cat lvgl-patches/series)` there; devicelab copies them to
`aera/build/patches/external/lvgl/`.

| Patch | What |
| --- | --- |
| 0001 | `src/draw/sw/blend/lv_draw_sw_blend.c`: clip every software blend to the target layer's buffer, so a clip area that reaches past it skips the draw instead of writing through NULL or past a row (backstop for devicelab D10/D12) |
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
