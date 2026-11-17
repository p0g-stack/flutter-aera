# flutter-aera

The Flutter embedder for AERA Recovery, in Rust. What `shell/platform/linux`
is to GTK, this is to AERA: it hosts `libflutter_engine.so`, renders through GL
(Zink), Vulkan or Impeller, hands frames to AERA, and answers Flutter's standard
platform channels from AERA's input. A stock app runs unmodified.

This branch targets upstream **Host API 2 desktop surfaces** at recovery
`1cabc509`, with optional negotiated extensions for desktop keyboard geometry,
Unicode entry and native Files/editor overlays. It does not use the colliding
Host API 3 wire protocol. See [`spec/host.md`](spec/host.md) and the
[feasibility and qualification report](docs/upstream-host-api-2.md).
Plugins run as root in recovery's namespaces; there is no helper or sandbox.

## Parity reference

AERA is a Linux process in recovery, so its peers are Linux GTK and
flutter-pi, not Android. Recovery has fewer things to reach parity with; what
AERA can't do is answered as not-implemented on the standard channel, never a
custom channel. Padding (rounded corners) rides the window metrics as
`viewPadding`, through our engine patch (`third_party/flutter-engine/`), as
Android reports its bars and cutout. AERA exposes no corner geometry at
runtime, so each edge comes from, first match wins: `$AERA_PLUGIN_DATA/view.json`
(tuning override), the payload's `usr/share/flutter/view.json` (written by
flutter_p0g from `aera/plugin.json` `"padding"`), AERA's givens (top 0: the
status bar is above the surface), our default (4 dp sides, 8 dp bottom).
Format: `{"padding": {"left": 4, "top": 0, "right": 4, "bottom": 8}}` in dp,
any edge may be left out. See `src/padding.rs`.

## Scope

In: engine hosting, renderers, the AERA host transport and its spec, the
standard-channel handlers, the static launcher, the host simulator, engine /
Mesa / runtime-kit builds (CI), an example `.aerap`.

Out: anything an app links, the app-developer tool (`flutter_p0g`, which
builds, packs and runs `.aerap`s and adds the `aera/` platform folder), app code.

## Nest

```
spec/host.md          upstream API 2 + negotiated desktop extension contract
spec/engine-pin.md    Flutter release, engine revision, header provenance
src/engine.rs         dlopen, proc table, project args, run          ≈ fl_engine
src/view.rs           window metrics and frame slots                ≈ fl_view
src/task_runner.rs    platform task runner, estimated vsync with ACK backpressure
src/renderer/         gl.rs (default), vk.rs (--vulkan); Skia or Impeller; readback now, dma-buf later
src/host/             SCM_RIGHTS control, desktop Play transport, memfd slots and input
src/handlers/         one per standard channel
src/ime.rs            text-input model
src/env.rs            XDG dirs inside AERA_PLUGIN_DATA, ALSA config in the payload
src/audio.rs          starts AERA's audio bridge, as AERA's own features do
audio/                ALSA's default device for the payload: AERA's audio bridge
launcher/             static aera-plugin: private mount ns, font + CA binds, exec via payload ld-linux
sim/                  aera-host-sim: AERA's side on a PC
vendor/               AERA protocol.hpp (as published and as patched) + flutter_embedder.h
third_party/aera/patches-api2/  small opt-in extensions against upstream 1cabc509
third_party/mesa/     Zink-on-KGSL surfaceless patch
ci/                   engine + gen_snapshot, Mesa, runtime-kit build scripts
example/              the counter app as an .aerap
```

## Status

The branch's reproducible host-only proof is `ci/api2-check.sh`: it runs the
Rust workspace tests, then the same Flutter probe under unmodified API 2
simulation and the explicitly negotiated extensions. It checks counter taps,
different before/after screenshots, route Back, Unicode text submission,
keyboard insets and a native Files round trip. Results and limitations are
recorded in `docs/upstream-host-api-2.md`.

Use the SDK matching the unchanged engine artifact (Flutter 3.47.5). The
parallel Flutter/kit bump is a separate track; this branch does not change
consumer pins. The old API 3 simulator scripts and recovery patch series are
historical and are not this branch's qualification entry points.

```sh
FLUTTER=/path/to/flutter-3.47.5/bin/flutter ci/api2-check.sh /tmp/api2-proof
```

This is not yet a qualified Cuttlefish image or a released API 2 runtime kit.
Do not flash, unlock or select any physical device. Cuttlefish qualification
must use upstream recovery plus the explicitly listed remaining patches.

## License

LGPL-3.0-or-later with the LGPL-3.0 linking exception
(`LICENSE`, `LICENSE.exception`; SPDX `LGPL-3.0-or-later WITH LGPL-3.0-linking-exception`).
Apps may link this library statically or dynamically, private apps included,
without releasing their own code or shipping relinking material. Changes to
the library itself stay LGPL.
