# Draft issue: Host API 3, plugins that draw their own pixels

**Where:** AERA-Recovery/android_bootable_recovery, issue before any PR.

**What:** Host API 2 plugins describe a declarative UI that AERA draws.
Some apps need to draw their own frames: a Flutter app, a game, a
video player. Host API 3 adds that for generic plugins without changing
Host API 2.

**How it works (patch H3 in this folder):**
- A plugin opts in with `protocol_version` 3 and the `pixel-surface`
  permission. It is still a generic ui-runtime payload running
  `usr/bin/aera-plugin`.
- AERA hands it a sealed memfd of frame slots on fd 3
  (`AERA_SURFACE_FD`) and a `SURFACE` message with the size, stride and
  scale. The plugin draws into a free slot and sends `PRESENT`; AERA
  shows it and answers `FRAME_DONE` when the slot is free again. AERA
  copies nothing it does not show.
- Touches, keys and the on-screen keyboard come over the existing
  control socket as new message kinds; Back goes to the plugin first.
- Each plugin gets a private data directory (`AERA_PLUGIN_DATA`) that
  survives updates and is removed with the plugin.
- The scene keeps the plugin alive across a rotation (a new `SURFACE`
  on the same memfd), pauses it when left (no `FRAME_DONE`) and resumes
  it from Recents, passes AERA's light or dark appearance, and lets it
  pick files through AERA's shared file picker (wave 2 patch 0018).
- The RPC gains `plugin`/`open` so a developer can start an installed
  plugin over adb.

**Running today:** a Flutter embedder (p0g-stack/flutter-aera) on a
OnePlus 15 (Adreno 840, Turnip + zink) and on Cuttlefish.

**Questions for maintainers:**
1. Is a pixel surface something AERA wants for plugins at all, or only
   for built-in scenes?
2. Message numbers: kinds 12-14 and 68-75 are taken in H3. Fine, or
   reserve a range?
3. Should the data directory and the file picker go in separately
   first?
4. One squashed PR (H3, about 2000 lines with tests and docs) or the
   15 smaller patches it was made from?
