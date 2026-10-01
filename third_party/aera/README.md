# AERA Host API 3

A real Host API 3 for AERA Recovery, carried as a patch series against a
pinned AERA commit (the way the frb patches are carried) until it is offered
upstream.

pin: abf33169b27dee17123c3c436a7299427821b329

Upstream: <https://github.com/AERA-Recovery/android_bootable_recovery>
(`bootable/recovery` in AERA's manifest). The patches touch `aeraui/`, `aera_rpc/`
(0008) `prebuilt/` (0010) and `recovery_utils/` (0012).

Apply with `git am $(cat patches/series)` on a checkout of the pin. Devicelab
copies `patches/*.patch` to `aera/build/patches/bootable/recovery/` for the
Cuttlefish image.

| Patch | What |
| --- | --- |
| 0001 | wire: version 3, kinds 12–14 and 68–75, features, surface constants |
| 0002 | session: per-scene version, `SURFACE` before `RESUME`, `PRESENT` outside the rate limit |
| 0003 | `plugin_api/surface.*`: sealed memfd slots and who owns each |
| 0004 | launcher: surface on fd 3, `AERA_SURFACE_FD`, `AERA_PLUGIN_DATA`, `--aera-host-api=3` |
| 0005 | plugin manager: v3 manifests (`pixel-surface` …), data directory |
| 0006 | `scenes/pixel_plugin_scene.cpp`, routing, the engine's pointer hook |
| 0007 | tests and the Host API 3 section of `plugin_api/README.md` |
| 0008 | AERA RPC `plugin` / `open`: start an installed plugin over adb, for `flutter attach` (docs/debugging.md) |
| 0009 | `AERA_APPEARANCE=light\|dark` in a Host API 3 plugin's environment, from AERA's theme (platform brightness) |
| 0010 | `prebuilt/Android.mk`: pack `libincfs.so` on every build (`libandroidfw.so` needs it), not only with FBE |
| 0011 | libwebp's SSE2/SSE4.1 sources, so recovery links for x86_64 (Cuttlefish) |
| 0012 | `recovery_utils/battery_utils.cpp`: wait at most 5 s for a declared health HAL, then fall back to the defaults (devicelab D2) |

What differs from our first guess (aera-flutter-demo#1):
`HELLO_ACK` carries the version in `value` and the features in `flags`, as
Host API 2 already does, and a slot comes back with `FRAME_DONE` only once a
newer frame has replaced it on screen (or at once if a newer `PRESENT`
superseded it before it was shown). The embedder and the sim follow both.

Checked by `ci/aera-host-check.sh` (session and surface unit tests, then the
counter through AERA's launcher, session and surface). The LVGL scene is not
run in CI: it builds only inside the AERA tree.

Not yet: resize or rotation (a rotated scene restarts the plugin), dma-buf
slots, damage rectangles, audio. Every known AERA gap, fixed or worked
around, is in `docs/aera-deficiencies.md`.

Licence: the patches are AERA's code and stay under its Apache-2.0.
