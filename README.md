# flutter-aera

The Flutter embedder for AERA Recovery, in Rust. What `shell/platform/linux`
is to GTK, this is to AERA: it hosts `libflutter_engine.so`, renders through GL
(Zink), Vulkan or Impeller, hands frames to AERA, and answers Flutter's standard
platform channels from AERA's input. A stock app runs unmodified.

It targets AERA's **generic pixel + GPU plugin host** (Host API 3). That host
is not published yet, so we build against an assumed interface
([aera-flutter-demo#1](https://github.com/1vivy/aera-flutter-demo/issues/1));
every assumption is marked `ASSUMED` until AERA publishes. Plugins run as root
in recovery's namespaces, so apps get root in-process; there is no helper.

## Parity reference

AERA is a Linux process in recovery, so its peers are Linux GTK and
flutter-pi, not Android. Recovery has fewer things to reach parity with; what
AERA can't do is answered as not-implemented on the standard channel, never a
custom channel.

## Scope

In: engine hosting, renderers, the AERA host transport and its spec, the
standard-channel handlers, the static launcher, the host simulator, engine /
Mesa / runtime-kit builds (CI), an example `.aerap`.

Out: anything an app links, the app-developer tool (`flutter_p0g`, which
builds, packs and runs `.aerap`s and adds the `aera/` platform folder), app code.

## Nest (proposed)

```
spec/host.md          Host API 3 messages we rely on, ASSUMED items marked
spec/engine-pin.md    Flutter release, engine revision, header provenance
src/engine.rs         dlopen, proc table, project args, run          ≈ fl_engine
src/view.rs           window metrics, pointer, frame slots           ≈ fl_view
src/task_runner.rs    platform task runner, vsync from FRAME_DONE
src/renderer/         gl.rs (Zink), vk.rs, impeller; readback now, dma-buf later
src/host/             socket + memfd slots, SURFACE, input, lifecycle, keyboard inset
src/handlers/         one per standard channel
src/ime.rs            text-input model
launcher/             static aera-plugin: private mount ns, font + CA binds, exec via payload ld-linux
sim/                  aera-host-sim: AERA's side on a PC
vendor/               AERA protocol.hpp + flutter_embedder.h, fetched by hash
third_party/mesa/     Zink-on-KGSL surfaceless patch
ci/                   engine + gen_snapshot, Mesa, runtime-kit build scripts
example/              the counter app as an .aerap
```

## License

LGPL-3.0-or-later with the LGPL-3.0 linking exception
(`LICENSE`, `LICENSE.exception`; SPDX `LGPL-3.0-or-later WITH LGPL-3.0-linking-exception`).
Apps may link this library statically or dynamically, private apps included,
without releasing their own code or shipping relinking material. Changes to
the library itself stay LGPL.
