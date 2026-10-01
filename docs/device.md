# Running the kit on a device without AERA's host

Until AERA ships its generic pixel host, `aera-host-sim` stands in for it on
the device: it creates the frame memfd and control socket exactly as
spec/host.md assumes, starts `aera-plugin` with them on fds 3 and 4, answers
the handshake, releases frames on a 60 Hz clock, replays scripted taps and
writes frames as PNGs. Everything below the host is the real thing: the
static launcher, the payload's own glibc loader, the engine, Mesa and the
device's GPU, inside recovery's environment.

What this does not exercise: AERA's plugin manager (install, payload
extraction, scene lifecycle) and AERA compositing our frames on screen.

## Files (release `kit-3.47.5`)

| Asset | What |
| --- | --- |
| `flutter-aera-kit-linux-<arch>-debug-3.47.5.tar.xz` | the runtime kit, `arm64` or `x64` |
| `aera-host-sim-linux-<arch>` | the stand-in host, static (no loader needed, runs on bionic recovery) |
| `counter-flutter_assets-3.47.5.tar.xz` | the default counter app, debug kernel bundle (architecture independent) |

Each has a `.sha256` next to it.

## Run

On the computer (pick `arch` to match the device):

```sh
arch=arm64   # or x64
base=https://github.com/p0g-stack/flutter-aera/releases/download/kit-3.47.5
curl -fsSLO $base/flutter-aera-kit-linux-$arch-debug-3.47.5.tar.xz
curl -fsSLO $base/aera-host-sim-linux-$arch
curl -fsSLO $base/counter-flutter_assets-3.47.5.tar.xz
mkdir root && tar -C root -xJf flutter-aera-kit-linux-$arch-debug-3.47.5.tar.xz && rm root/kit.json
tar -C root/usr/share/flutter -xJf counter-flutter_assets-3.47.5.tar.xz
adb root
adb push root /tmp/aera-flutter
adb push aera-host-sim-linux-$arch /tmp/aera-host-sim
adb shell chmod 755 /tmp/aera-host-sim /tmp/aera-flutter/usr/bin/* /tmp/aera-flutter/usr/lib/ld-linux-*
```

Then on the device, from recovery's root shell:

```sh
adb shell '/tmp/aera-host-sim --root /tmp/aera-flutter --embedder /tmp/aera-flutter/usr/bin/aera-plugin \
  --out /tmp/aera-sim --gpu --until 8000 \
  --snap start@3000 --tap 980,2250@3200 --tap 980,2250@3500 --tap 980,2250@3800 --snap after@6000' \
  2>&1 | tee device-run.log
adb pull /tmp/aera-sim
```

`--size WxH --scale S` match the panel if wanted (default 1080x2400 at
2.75; the tap positions above assume it). Drop `--gpu` to force softpipe,
which isolates GPU problems from everything else.

## What to report back

- `device-run.log`, in particular the lines `aera-plugin: …` (mount
  namespace, binds), `aera-flutter: GL renderer …` (which Mesa driver ran),
  `sim: first frame after … ms` and the frame count.
- `aera-sim/start.png`, `after.png`, `last.png` (the counter should read 3).
- `ls -l /dev/dri /dev/kgsl-3d0 /twres/fonts` and `uname -m` from recovery.
- On failure: the same run with `MESA_DEBUG=1 EGL_LOG_LEVEL=debug` set in
  the `adb shell` command.

## GPU paths

| Device | Node | Mesa driver | Set by |
| --- | --- | --- | --- |
| Qualcomm phone | `/dev/kgsl-3d0` | Zink on Turnip | `aera-plugin` (`MESA_LOADER_DRIVER_OVERRIDE=zink`, `VK_DRIVER_FILES`) |
| Cuttlefish gfxstream modes (x86_64) | `/dev/dri/renderD128`, gfxstream Vulkan capset, no virgl | Vulkan renderer on gfxstream (`--vulkan`); GL through Zink does not start yet and falls back to softpipe | `aera-plugin` (capset probe) |
| Cuttlefish `--gpu_mode=drm_virgl` | `/dev/dri/renderD128` | virgl | Mesa's own probe |
| Cuttlefish `guest_swiftshader`, or none | — | softpipe | Mesa's fallback |

`aera-plugin` logs `virtio-gpu capsets 0x…` when there is a virtio-gpu
render node. On devicelab's Cuttlefish the gfxstream modes offer capsets 3
(gfxstream Vulkan), 9 (composer) and, in plain gfxstream, 8 (GLES), and
`drm_virgl` does not boot, so gfxstream is the GPU route there.

Proven on devicelab's Cuttlefish (run 36833432698): `--vulkan`, with Skia
and with Impeller, on "Virtio-GPU GFXStream (SwiftShader Device)". GL
there tried Zink and got "egl: failed to create dri2 screen", so the
launcher now passes `--vulkan` on that GPU; `--gl` in `engine-switches`
forces GL back. To see why Zink fails, put Mesa's debug variables in
`$AERA_PLUGIN_DATA/environment`, one `KEY=VALUE` per line:

```sh
adb shell 'printf "EGL_LOG_LEVEL=debug\nMESA_DEBUG=1\nMESA_LOG_LEVEL=debug\n" > /sdcard/AERA/plugin-data/ID/environment'
```

`usr/bin/aera-plugin --vulkan-info` (from `adb shell`, in the plugin's
directory) prints what the Vulkan driver offers through the payload's own
loader and ICDs: versions, extensions, features and limits per device, as
`vulkaninfo` would. Zink checks these before it runs on a device and does
not say which one failed in a release build.

GL is the default elsewhere. `--vulkan` in `$AERA_PLUGIN_DATA/engine-switches` (one
switch per line, `docs/debugging.md`) renders with Vulkan instead: Turnip
on a Qualcomm phone, gfxstream on x64 Cuttlefish; where Vulkan cannot start
it falls back to GL and logs why. `--enable-impeller` switches either to Impeller.
The log names the result: `aera-flutter: GL renderer …` or `Vulkan renderer
…`, and the engine's `Using the Impeller rendering backend (…)`.
