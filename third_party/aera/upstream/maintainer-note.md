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

**Fix.** lvgl#6 and lvgl#7 are withdrawn. The crash lvgl#6 was fixing (an object
shrunk on press near a screen edge made the software renderer blend
outside the display buffer, SIGSEGV in `lv_draw_sw_blend`) is fixed
instead by a one-line clip in `lv_draw_sw_blend.c`
(`lvgl/0001-draw-sw-never-blend-outside-the-target-buffer.patch`).
It leaves transforms exactly as stock draws them.

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
- an older image without any LVGL patch (run 37518367652): the fastboot
  transition runs, recovery restarts into fastboot and comes back, and
  the UI keeps drawing
- image with only the new 0001 (run 37533988207): the held Files card
  keeps its size and place and only changes colour; the fastboot
  transition animates and the UI keeps drawing and answering (recovery
  idle, 5 of 200 ticks); the power menu opens and Recovery logs
  "Rebooting...". Cuttlefish itself did not restart afterwards (uptime
  kept rising), which we read as the virtual device, not AERA: with
  lvgl#6 it never got as far as "Rebooting...". The crash 0001 is for
  (Recents card of a paused plugin, twice) and a rotation both pass with
  recovery in the same process and no segfault

Any patch offered again comes with its stock-UI run.
