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

## Worked around in the payload

| Gap | Workaround | Upstream fix would be |
| --- | --- | --- |
| Plugin stdout and stderr go to recovery's own streams, which nobody reads | `aera-plugin` redirects both to `$AERA_PLUGIN_DATA/aera-flutter.log` | a per-plugin log AERA keeps (and shows) |
| Fixed plugin environment, no developer hook | `aera-plugin` reads `$AERA_PLUGIN_DATA/environment` and `engine-switches` | a debug environment in the manifest or over RPC |
| Fonts only at `/twres/fonts`, no fontconfig | `aera-plugin` bind-mounts them at `/usr/share/fonts` in a private namespace | a font path in the plugin environment |
| No CA bundle for plugins | the payload ships one, bound at `/etc/ssl/certs` | a system CA path in the plugin environment |
| No GPU driver for plugins; the GPU stack is the recovery's, not ours | the payload carries Mesa (Turnip, gfxstream, Zink) and picks it in `aera-plugin` | none; a plugin should bring its own userspace |

## Not covered yet

- Resize and rotation: a rotated scene restarts the plugin instead of
  sending a new `SURFACE`.
- Slots are sealed memfds copied by the CPU; no dma-buf slots, so a GPU
  frame is read back before AERA sees it.
- No damage rectangles on `PRESENT`: AERA redraws the whole surface even
  when we copied only what changed (`src/renderer/damage.rs`).
- No audio.
