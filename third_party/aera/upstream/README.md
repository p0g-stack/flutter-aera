# AERA upstream exports

Patches prepared for AERA-Recovery/android_bootable_recovery (branch
`aera-16.0`, head 92813466) and android_external_lvgl (`aera-16.0`, head
212fd3a). Each was checked with `git am` on that head; "standalone" means
it applies alone. Subjects follow upstream's `area: lowercase imperative`.
The author is still `Claude <noreply@anthropic.com>` and the bodies keep
their existing trailers: the human author and any disclosure line are
Yuv's decision, set before sending.

Hardware status: **a56b3ac** = same patch-id as in image
aera-infiniti-arm64-a56b3ac-ca015146c92db600, tested on the Infiniti.
**1793c04** = only in aera-infiniti-arm64-1793c04-508a5ee16d19acf7, not
yet tested on hardware.

## wave1/ (standalone fixes)

| Patch | Source | Hardware | Apply on head |
| --- | --- | --- | --- |
| 0010 prebuilt: pack libincfs.so on every build | patches/0010 | a56b3ac | standalone |
| 0013 minuitwrp: scan out on the CRTC's primary plane without an SDE topology | patches/0013 | a56b3ac | standalone |
| 0014 prebuilt: pack task_profiles.json on every build | patches/0014 | a56b3ac | standalone |
| 0016 aeraui: take an RPC request whose writer already hung up | patches/0016 | a56b3ac | standalone, compiles |
| 0025 minuitwrp: start a new contact where its slot last was | patches/0025 | a56b3ac | standalone |
| 0026 aeraui: draw landscape on the software renderer | patches/0026 | a56b3ac | standalone, compiles |
| R0001 aera_remote: create the virtual input device when Remote starts | remote-patches/R0001 | a56b3ac | standalone |
| R0002 aera_remote: answer /screen.jpg with a frame taken after the request | remote-patches/R0002 | a56b3ac | standalone |
| R0003 aeraui: act on the Home and Menu keys | remote-patches/R0003 | 1793c04 (a56b3ac had the 0024 variant) | standalone, compiles |
| C0001 aeraui: build libwebp's SSE2 and SSE4.1 sources (optional) | cuttlefish-patches/C0001 | Cuttlefish builds only | standalone |

## wave2/ (needs maintainer agreement)

| Patch | Source | Hardware | Apply on head |
| --- | --- | --- | --- |
| 0018 aeraui: add a shared file picker with Telegram as its first caller | patches/0018 (subject reworded) | a56b3ac | standalone, compiles |
| F0001 aeraui: start the Recents swipe on movement, not on touch-down | refinement-patches/F0001, fresh export | 1793c04 ran it with pixel plugins; this standalone form (no pixel-plugin branch) is new and untested | standalone, compiles |

## wave3/ (design issue first)

| File | Source | Hardware | Apply on head |
| --- | --- | --- | --- |
| H3 plugin_api: add Host API 3 pixel plugins | patches/0001-0009, 0015, 0019-0022, 0029 squashed | a56b3ac (every part has the same patch-id) | on head + wave2 0018; tree equals the series |
| design-issue.md | draft issue text | | |

## lvgl/ (android_external_lvgl; 0003 is also a candidate for lvgl/lvgl)

| Patch | Source | Hardware | Apply on head |
| --- | --- | --- | --- |
| 0002 aera: draw transforms through layers, not the unsupported matrix | lvgl-patches/0002 | a56b3ac | applies on 212fd3a |
| 0003 draw: allocate a layer larger than the whole budget when none is held | lvgl-patches/0003 | a56b3ac | applies on 212fd3a |

"Compiles" = `-fsyntax-only` of the touched aeraui sources against
stock LVGL with stubs; minuitwrp, prebuilt and aera_remote changes were
not compiled here (they are in both images' builds).
