# AERA deficiencies

What AERA Recovery (at the pin in `third_party/aera/README.md`) lacks for a
pixel plugin like this embedder, and what we do about each. A gap fixed in
AERA gets a patch in `third_party/aera/patches`, written to go upstream:
AERA's style, one concern per patch, the message explaining the gap. A gap
we only work around says where, so the workaround can go once AERA fixes it.

New gaps found on the Cuttlefish image (devicelab `aera/`) are added here
and, where they belong in AERA, answered with the next patch number.

## Fixed by the series

| Gap | Patch |
| --- | --- |
| Plugins can only describe declarative UI (Host API 2); no way to hand AERA pixels | 0001, 0003, 0006 |
| One protocol version for every plugin; 128 messages/s limit counts every frame | 0002 |
| No way to pass a plugin its surface at launch | 0004 |
| No private, writable data directory per plugin | 0004, 0005 |
| Manifests cannot declare pixel-surface plugins | 0005 |
| Touch, keyboard and Back reach only AERA's own widgets | 0006 |
| A plugin can only be started from the on-device UI, not over adb (no `flutter attach`) | 0008 |
| Plugins cannot follow AERA's light or dark theme | 0009 |
| Recovery does not start without FBE: `libandroidfw.so` needs `libincfs.so`, packed only for FBE builds (devicelab D1) | 0010 |
| No x86_64 build: libwebp is built with NEON sources only, so recovery fails to link | 0011 |
| The battery monitor re-queries the health HAL every second, so without one it is two failed service manager lookups per second (log spam, wasted binder calls) (devicelab D2) | 0012 |
| Blank screen on any non-Qualcomm DRM driver: the display code splits the screen across the first two planes, and on virtio-gpu the second is a cursor plane, so every atomic commit fails with EINVAL (devicelab D3) | 0013, confirmed on Cuttlefish image e54e216 |
| logd aborts every few seconds on builds that ship logd (`TARGET_USES_LOGD := true`, as Cuttlefish does): `/etc/task_profiles.json` is only required with encryption and never copied into the ramdisk (devicelab D4) | 0014, confirmed on Cuttlefish image e54e216 |
| Touches that start within 72 px of a side edge never reach a pixel plugin (our 0006 copied Browser's rule), so a FAB near the edge is dead; AERA's own widgets get them (devicelab D5) | 0015, confirmed on Cuttlefish image 29c34fe |
| An RPC request whose writer closes between two frames sits unread until the next client writes, which then gets the first answer and loses its own (stalled second `plugin open`, AERA Remote not starting) (devicelab D6) | 0016, confirmed on Cuttlefish image 29c34fe |
| AERA Remote's first touch after start is lost: the uinput device is created on that touch, recovery rescans /dev/input at most every 2 s, so only the release arrives (two quick taps opened Quick Settings) (devicelab D7) | 0017 |
| AERA Remote's Home and Menu buttons do nothing: they send KEY_HOMEPAGE and KEY_MENU, which recovery never handled (only Power, Volume, Back), so Home and Recents were reachable only by the bottom-edge swipe (devicelab D8, found in source) | 0023 |
| A tap at the same x as the previous one arrives at x=0 (the left Back edge), and one on the same spot not at all: the touch translator zeroes its position on release and the kernel never re-sends an unchanged ABS_MT_POSITION_X/Y (the second tap into a plugin was lost; AERA Remote hits it often) (devicelab D9) | 0025 |
| With `/data` not mounted, `/sdcard/AERA` can still exist as a folder in recovery's RAM root, so plugin data went there without `AERA_PLUGIN_DATA_VOLATILE=1` and was lost on reboot (devicelab D11) | 0005 uses `/sdcard/AERA` only when it is on a different filesystem from `/` and not tmpfs or ramfs; otherwise RAM, flagged volatile by 0022 |
| A Recents card or tile row that runs past a screen edge while shrunk on press crashes recovery (SIGSEGV in `lv_draw_sw_blend`, NULL destination) when it is redrawn in full: opening a paused pixel plugin from Recents, or rotating back from landscape. AERA's LVGL enables `LV_DRAW_TRANSFORM_USE_MATRIX`, which neither of its renderers (software, OpenGL ES) supports: transforms were never drawn, and the widened clip let off-screen children blend outside the buffer (devicelab D10, D12's crash) | LVGL 0002 turns the option off, so transforms draw through layers and actually scale; LVGL 0001 clips every software blend to its buffer as a backstop. Verified on image 37c931a (D10 A and B, rotate) |
| On the software renderer (no Adreno), landscape froze on the portrait frame: LVGL drew a 1348-wide layout into the panel's 720-pixel-stride buffers, folding rows, and only redrew what changed (devicelab D12) | 0026 draws landscape into its own buffer and turns each frame upright on flush. Verified on image 3f593bb, with the counter drawn and tappable in landscape (embedder bcd5e6e) and rotate back clean |
| With adaptive resolution on (logical 1440x2696 on the 720x1348 panel), tapping a paused plugin's Recents card hung recovery: the process stayed alive but drew and answered nothing. A card shrunk on press is drawn through a transform layer (LVGL 0002), which at that size exceeds AERA's 4 MiB layer budget, and LVGL waited forever for memory no other layer held (devicelab D13) | LVGL 0003 allocates such a layer when no other layer holds memory. Reproduced and fixed on a host build of AERA's LVGL (1440x2696, card 1200x2200 shrunk: hangs before, drawn after) |
| AERA Remote's `/screen.jpg` returned the frame from the previous request: captures run only while a client asks, so after a pause (or a rotation) a single screenshot was stale (devicelab D12) | 0027 waits for a capture taken after the request. Verified on image 3f593bb |
| The file picker opens in TWRP's current storage, which on a data partition Android never booted is `/data/media` with no user folder yet: "Cannot open folder" and an empty list (devicelab, image with 0023; Files starts there too) | 0018 falls back to `/data/media/0`, then `/sdcard`, when the start cannot be listed |

## Worked around in the payload

| Gap | Workaround | Upstream fix would be |
| --- | --- | --- |
| Plugin stdout and stderr go to recovery's own streams, which nobody reads | `aera-plugin` redirects both to `$AERA_PLUGIN_DATA/aera-flutter.log` | a per-plugin log AERA keeps (and shows) |
| Fixed plugin environment, no developer hook | `aera-plugin` reads `$AERA_PLUGIN_DATA/environment` and `engine-switches` | a debug environment in the manifest or over RPC |
| Fonts only at `/twres/fonts`, no fontconfig | `aera-plugin` bind-mounts them at `/usr/share/fonts` in a private namespace | a font path in the plugin environment |
| No CA bundle for plugins | the payload ships one, bound at `/etc/ssl/certs` | a system CA path in the plugin environment |
| No GPU driver for plugins; the GPU stack is the recovery's, not ours | the payload carries Mesa (Turnip, gfxstream, Zink) and picks it in `aera-plugin` | none; a plugin should bring its own userspace |

## Fixed outside AERA's tree

- x86_64 also needs `system/core` to keep `libcutils/arch-x86_64/cache.h`,
  which AOSP dropped after Android 13 while this tree still builds the
  x86_64 `android_memset` sources that include it. That fix belongs to the
  `system/core` AERA builds against, not to `bootable/recovery`; devicelab
  carries it (`aera/build/patches/system/core`), restoring the header from
  android-13.0.0_r1.
- Cuttlefish device-tree settings (`PRODUCT_BUILD_RECOVERY_IMAGE`, build
  type, maximum brightness) are configuration, kept in devicelab.

## Not covered yet

- Slots are sealed memfds copied by the CPU; no dma-buf slots, so a GPU
  frame is read back before AERA sees it.
- No damage rectangles on `PRESENT`: AERA redraws the whole surface even
  when we copied only what changed (`src/renderer/damage.rs`).
- Sound needs AERA's closed `aera-audio-bridge`, which only official AERA
  builds ship: images built from AERA's source (ours on Cuttlefish) have no
  sound.
