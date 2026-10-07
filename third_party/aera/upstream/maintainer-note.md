# Note for AERA's maintainer (2026-10-06)

Thanks for testing, and sorry: you were right, and the cause is one
patch.

**Cause.** lvgl#6 (android_external_lvgl#6) sets `LV_DRAW_TRANSFORM_USE_MATRIX` to
0. AERA's renderers (software and OpenGL ES) never apply a draw task's
matrix, so with the option at 1 every `transform_scale` in AERA's UI is
drawn at 100%, and the UI is tuned to that. Turning it off makes all of
them real at once: pressed buttons and cards (scale 238-253 with the
default top-left pivot, so they also shift), enlarged icons (352-520),
the mode transition's content scaling and the power transition's logo.
Those now draw through transform layers, and the full-screen ones in the
transitions wait on AERA's 4 MiB layer budget, so a transition (and the
reboot that ends the power transition) can stall. lvgl#7 only existed to
make lvgl#6's layers fit. We tested plugins on top of lvgl#6 and never compared
AERA's own UI against stock, which is how this got through.

**Fix.** lvgl#6 and lvgl#7 are withdrawn, and nothing goes to your LVGL
fork. Your `lv_conf.h` enables `LV_DRAW_TRANSFORM_USE_MATRIX`, which
neither of AERA's renderers applies (upstream ships it off: it is for
renderers that apply matrices). LVGL then widens the clip area of a
scaled object without drawing it scaled, and near a layer's edge the
software blend writes outside its buffer. The new recovery patch
`wave1/0030-aeraui-clip-every-draw-task-to-its-layer-s-buffer.patch`
adds, in `aeraui/core/engine.cpp`, a draw unit that takes no tasks and
clips each new draw task to its layer's buffer before the real units
see it. Nothing visible changes, and transforms stay exactly as stock
draws them. On Cuttlefish, without it the fastboot transition crashes
recovery; with it, it does not (results below). Upstream LVGL master has
no such clip, so a rebase of the fork would not bring one.

**The recovery PRs** do not touch drawing, the power menu or reboot:
recovery#4 and #6 packaging, #5 only without an SDE topology, #7 RPC,
#8 touch slots, #9 software renderer in landscape, #10-#12 AERA Remote
and the Home/Menu keys, #13 Cuttlefish build only (#14-#16 are the
file picker, Recents swipe and Host API 3). They were rejected together,
but none of the symptoms comes from them.

**How it is tested now.** Our Cuttlefish lab runs AERA's own UI with no
plugin on every image: hold a Home card (frames before, held and after),
fastboot transition and back, and power menu -> Recovery (checks that
the device actually rebooted). Results:

- image with lvgl#6/#7 (Cuttlefish, twice, runs 37518177513 and
  37523748507): the held Files card shrinks toward its top-left corner
  (right and bottom edges move in, left and top stay); after the fastboot
  transition recovery draws nothing more, stops answering, and spins one
  core at 100% (201 of 200 ticks in 2 s); the power menu never shows and
  the device never reboots
- images without any LVGL patch: on the current recovery series (run
  37565476825) and on an older one (run 37518367652), recovery died with
  SIGSEGV during the fastboot transition (no plugin involved; init
  restarted it, so the UI came back under a new pid). We first read the
  older run's new pid as a normal restart into fastboot; it was this
  crash. The Recents-card crash 0001 was first written for no longer
  happens on the current series without it
- image with an LVGL clip (run 37533988207), an earlier form of the
  same fix: the held Files card
  keeps its size and place and only changes colour; the fastboot
  transition animates and the UI keeps drawing and answering (recovery
  idle, 5 of 200 ticks); the power menu opens and Recovery logs
  "Rebooting...". Cuttlefish itself did not restart afterwards (uptime
  kept rising), which we read as the virtual device, not AERA: with
  lvgl#6 it never got as far as "Rebooting...". The crash this clip is for
  (Recents card of a paused plugin, twice) and a rotation both pass with
  recovery in the same process and no segfault
- image with the new 0030 and unpatched LVGL (run 37582507317): the
  same results as the line above: held card unchanged in size, fastboot
  transition in the same process with recovery idle afterwards (7 of
  200 ticks), power menu opens and Recovery logs "Rebooting...", the
  Recents-card checks and a rotation pass, no segfault

Any patch offered again comes with its stock-UI run.
