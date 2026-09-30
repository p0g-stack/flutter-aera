# flutter-aera

The Flutter embedder for AERA Recovery, in Rust. What `shell/platform/linux`
is to GTK, this is to AERA: it hosts `libflutter_engine.so`, renders through
GL (Zink) or Vulkan, hands frames to AERA over the bridge, and answers
Flutter's standard platform channels from AERA's inputs.

There is no Dart package. Because this embedder speaks the standard channels,
plain Flutter works unmodified: `WidgetsBindingObserver` for lifecycle,
`MediaQuery` for insets, `PopScope` for back, `Clipboard`, `platformBrightness`.

## Scope

In: engine hosting, renderers, the AERA bridge codec and its spec, the
standard-channel handlers, the host simulator, engine and runtime-kit builds,
`.aerap` packaging.

Out: anything an app links (`surfaces`), custom channels for things Flutter
already has a channel for.

## Proposed nest (mirrors the GTK embedder)

```
spec/
  bridge.md           transcribed from AERA's protocol.hpp; drift-tested against the vendored header
  engine-pin.md       Flutter release, engine revision, header provenance, runtime-mode files
src/
  engine.rs           dlopen, proc table, project args, run   ≈ fl_engine
  view.rs             window metrics, pointer, frame slots    ≈ fl_view
  task_runner.rs      platform task runner, vsync             ≈ fl_task_runner
  renderer/           RenderBackend trait; gl.rs, vk.rs       ≈ fl_compositor_*
  bridge.rs           AERA fd3/fd4 packet transport           ≈ fl_wayland_display
  handlers/           flutter/platform, settings, textinput, keyboard, lifecycle, navigation
  ime.rs              text-input state machine                ≈ common/text_input_model
  bin/host_sim.rs     AERA stand-in for CI and desk testing
vendor/               protocol.hpp and flutter_embedder.h, fetched by hash, revisions recorded
tools/                build_engine.sh, build_mesa.sh, assemble_runtime.py, make_aerap.py
third_party/mesa/     zink-kgsl-surfaceless patch
```

## Rules

- Standard channels only. A custom channel needs a written reason that no
  standard channel or window-metrics field covers the feature.
- AERA owns the bridge format. `vendor/protocol.hpp` is pinned by hash with
  the AERA commit recorded; a build-time test asserts `bridge.rs` against it.
- Readback offsets are aligned or the frame falls back to a full copy; never a
  silent skip.
- Every `unsafe` block states the invariant it relies on and who upholds it.
- CI runs the worker under the simulator on every PR and round-trips an `.aerap`.

## License

LGPL-3.0-or-later with the LGPL-3.0 linking exception
(`LICENSE`, `LICENSE.exception`; SPDX `LGPL-3.0-or-later WITH LGPL-3.0-linking-exception`).
Apps may link this library statically or dynamically, private apps included,
without releasing their own code or shipping relinking material. Changes to
the library itself stay LGPL.
