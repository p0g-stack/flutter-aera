# flutter-aera

The Flutter embedder for AERA Recovery, in Rust. What `shell/platform/linux`
is to GTK, this is to AERA: it hosts `libflutter_engine.so`, renders through GL
(Zink), Vulkan or Impeller, hands frames to AERA, and answers Flutter's standard
platform channels from AERA's input. A stock app runs unmodified.

It targets AERA's **pixel plugin host** (Host API 3). AERA has not published
one, so we wrote it: a patch series against AERA in `third_party/aera/`,
built into the devicelab Cuttlefish image and offered upstream from there.
`spec/host.md` is its contract. Plugins run as root
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

## Nest

```
spec/host.md          the Host API 3 contract (launch, handshake, frames, input)
spec/engine-pin.md    Flutter release, engine revision, header provenance
src/engine.rs         dlopen, proc table, project args, run          ≈ fl_engine
src/view.rs           window metrics, pointer, frame slots           ≈ fl_view
src/task_runner.rs    platform task runner, vsync from FRAME_DONE
src/renderer/         gl.rs (Zink), vk.rs, impeller; readback now, dma-buf later
src/host/             socket + memfd slots, SURFACE, input, lifecycle, keyboard inset
src/handlers/         one per standard channel
src/ime.rs            text-input model
src/env.rs            XDG dirs inside AERA_PLUGIN_DATA
launcher/             static aera-plugin: private mount ns, font + CA binds, exec via payload ld-linux
sim/                  aera-host-sim: AERA's side on a PC
vendor/               AERA protocol.hpp (as published and as patched) + flutter_embedder.h
third_party/aera/     Host API 3 for AERA, as patches against its pin
third_party/mesa/     Zink-on-KGSL surfaceless patch
ci/                   engine + gen_snapshot, Mesa, runtime-kit build scripts
example/              the counter app as an .aerap
```

## Status

The counter app renders and counts taps in `sim/` and through AERA's own
launcher, session and pixel surface with the Host API 3 patches
(`ci/aera-host-check.sh`), on the GL renderer with full-frame readback. The
debug runtime kits for arm64 and x64 are published (`spec/aerap.md`). Not
yet: a run inside an AERA image (devicelab's Cuttlefish build), a device
run, async/damage readback, Vulkan and Impeller, `example/` .aerap.

Next, in order: the Host API 3 image on Cuttlefish; then a debug run for
`flutter_p0g run aera` hot reload, flutter-pi style: a debug (JIT) engine in
the kit loading the app's `kernel_blob.bin`, the VM service URL in the log
(the debug engine already prints it) for port forwarding and
`flutter attach`, and a way to start a debug plugin from adb and from the
sim; then faster frame copies, Vulkan, Impeller. `spec/aerap.md` is the package layout `flutter_p0g` packs.

```sh
cargo test --workspace
ci/sim-smoke.sh /tmp/smoke      # needs flutter 3.47.5 on PATH; frames in /tmp/smoke/sim
sudo ci/aera-host-check.sh /tmp/smoke   # AERA + patches; frames in /tmp/smoke/aera-host
```

## License

LGPL-3.0-or-later with the LGPL-3.0 linking exception
(`LICENSE`, `LICENSE.exception`; SPDX `LGPL-3.0-or-later WITH LGPL-3.0-linking-exception`).
Apps may link this library statically or dynamically, private apps included,
without releasing their own code or shipping relinking material. Changes to
the library itself stay LGPL.
