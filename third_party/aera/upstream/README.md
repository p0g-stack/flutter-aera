# AERA upstream exports

Patches prepared for AERA-Recovery/android_bootable_recovery (branch
`aera-16.0`, head 92813466) and android_external_lvgl (`aera-16.0`, head
212fd3a). Each was checked with `git am` on that head; "standalone" means
it applies alone. Subjects follow upstream's `area: lowercase imperative`.
The files here keep `Claude <noreply@anthropic.com>` and their old
trailers; the submitted commits were re-authored as
`1vivy <1vivy@tutanota.com>` with `Assisted-by: Claude Opus 5.5` in
place of the `Co-Authored-By`/`Claude-Session` lines (same patch-ids).

Hardware status: **a56b3ac** = same patch-id as in image
aera-infiniti-arm64-a56b3ac-ca015146c92db600, tested on the Infiniti.
**1793c04** = only in aera-infiniti-arm64-1793c04-508a5ee16d19acf7, not
yet tested on hardware.

## Submitted 2026-10-05

From the forks p0g-stack/android_bootable_recovery and
p0g-stack/android_external_lvgl, one branch per patch on the heads above.
Upstream picks PRs by hand, so a picked patch shows up as a new upstream
commit, not a merge: drop it from the series at the next re-pin.

| Patch | Pull request |
| --- | --- |
| 0010 | [android_bootable_recovery#4](https://github.com/AERA-Recovery/android_bootable_recovery/pull/4) |
| 0013 | [android_bootable_recovery#5](https://github.com/AERA-Recovery/android_bootable_recovery/pull/5) |
| 0014 | [android_bootable_recovery#6](https://github.com/AERA-Recovery/android_bootable_recovery/pull/6) |
| 0016 | [android_bootable_recovery#7](https://github.com/AERA-Recovery/android_bootable_recovery/pull/7) |
| 0025 | [android_bootable_recovery#8](https://github.com/AERA-Recovery/android_bootable_recovery/pull/8) |
| 0026 | [android_bootable_recovery#9](https://github.com/AERA-Recovery/android_bootable_recovery/pull/9) |
| R0001 | [android_bootable_recovery#10](https://github.com/AERA-Recovery/android_bootable_recovery/pull/10) |
| R0002 | [android_bootable_recovery#11](https://github.com/AERA-Recovery/android_bootable_recovery/pull/11) |
| R0003 | [android_bootable_recovery#12](https://github.com/AERA-Recovery/android_bootable_recovery/pull/12) (body: not yet run on hardware) |
| C0001 | [android_bootable_recovery#13](https://github.com/AERA-Recovery/android_bootable_recovery/pull/13) |
| 0018 | [android_bootable_recovery#14](https://github.com/AERA-Recovery/android_bootable_recovery/pull/14) |
| F0001 | [android_bootable_recovery#15](https://github.com/AERA-Recovery/android_bootable_recovery/pull/15) (body: syntax-checked only, not built or run) |
| H3 | [android_bootable_recovery#16](https://github.com/AERA-Recovery/android_bootable_recovery/pull/16) (stacked on #14; body from wave3/pr-body.md) |
| lvgl 0002 | [android_external_lvgl#6](https://github.com/AERA-Recovery/android_external_lvgl/pull/6) (withdrawn 2026-10-06, see below) |
| lvgl 0003 | [android_external_lvgl#7](https://github.com/AERA-Recovery/android_external_lvgl/pull/7) (withdrawn 2026-10-06) |

All 15 were rejected on 2026-10-06 as untested: on the maintainer's build
AERA's own UI regressed (pressed buttons drawn far smaller and misaligned,
mode transitions and reboot not happening). The cause is lvgl 0002: it
turns `LV_DRAW_TRANSFORM_USE_MATRIX` off, so every `transform_scale` in
AERA's UI, tuned for renderers that ignore it, is drawn for the first
time and transform layers appear in the transitions. lvgl 0002 and 0003
are withdrawn; lvgl 0001 below replaces them. The recovery patches do not
touch drawing, the power menu or reboot (read for the stock path:
0010/0014 packaging, 0013 only without an SDE topology, 0016 RPC, 0025
touch slots, 0026 software renderer in landscape, R0001-R0003 Remote and
keys), and every hardware run listed here had lvgl 0002 in it.
See [maintainer-note.md](maintainer-note.md).

Owed upstream: hardware results for #12 (image 1793c04) and #15 (its
standalone form), posted on those PRs.

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

## wave3/

| File | Source | Hardware | Apply on head |
| --- | --- | --- | --- |
| H3 plugin_api: add Host API 3 pixel plugins | patches/0001-0009, 0015, 0019-0022, 0029 squashed | a56b3ac (every part has the same patch-id) | on head + wave2 0018; tree equals the series |
| pr-body.md | H3's PR body (draft) | | |

## lvgl/ (android_external_lvgl)

| Patch | Source | Hardware | Apply on head |
| --- | --- | --- | --- |
| 0001 draw/sw: never blend outside the target buffer | lvgl-patches/0001 | not yet: needs a Cuttlefish run (D10, stock UI) on an image built with it | applies on 212fd3a, compiles |

"Compiles" = `-fsyntax-only` of the touched aeraui sources against
stock LVGL with stubs; minuitwrp, prebuilt and aera_remote changes were
not compiled here (they are in both images' builds).
