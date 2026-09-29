# engine-aera

The Rust embedder that runs Flutter inside AERA Recovery. AERA spawns it as its
browser worker; it drives `libflutter_engine.so`, renders through GL (Zink) or
Vulkan, and hands finished frames to AERA over the bridge.

## Scope

In: the AERA bridge codec and its spec, the renderers, the host simulator, the
worker binary, engine and runtime-kit builds, `.aerap` packaging.

Out: the Dart side of the AERA channel (`surfaces/packages/surfaces_aera`),
anything an app links (`surfaces`).

## Proposed nest

```
spec/
  bridge.md           fd contract, packet layout, kinds, validation both ways, slot/ACK state machine
  channels.md         aera/system, flutter/platform, flutter/textinput: methods, replies, unsupported list
  engine-pin.md       Flutter release, engine revision, header provenance, runtime-mode files
crates/
  aera-bridge/        packet codec; asserted against AERA's protocol.hpp
  aera-render/        RenderBackend trait; gl and vk behind it; damage and alignment rules
  aera-testkit/       host simulator as a library, plus the CLI
  aera-ime/           pure text-input state machine
worker/               engine lifecycle, platform task runner, vsync, adapters; the binary
tools/
  build_engine.sh     pinned Flutter + pinned depot_tools
  build_mesa.sh       pinned Mesa + zink-kgsl-surfaceless patch
  assemble_runtime.py, make_aerap.py
third_party/mesa/
vendor/               flutter_embedder.h, fetched by hash for the pinned engine
```

## Rules the old code lacked

- The bridge format has one owner and a cross-implementation test.
- Readback offsets are aligned or the frame falls back to a full copy; never a
  silent skip.
- Every `unsafe` block states the invariant it relies on and who upholds it.
- CI runs the worker under the simulator on every PR and round-trips an `.aerap`.

## License

LGPL-3.0-or-later.
