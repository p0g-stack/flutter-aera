# Flutter engine patches

Patches against the pinned Flutter revision (`spec/engine-pin.md`), applied
by `ci/build-engine.sh` at the root of a flutter/flutter checkout of the pin
(`git am $(cat patches/series)`). The script builds the embedder engine the
way flutter-pi's engine builds do (ardera/flutter-ci): embedder only, minimal
Linux, no desktop shells. `ci/publish-engine.sh` uploads the result to the
release named by `ci/engine-tag.sh` (`engine-<revision>-<series hash>`), and
`ci/fetch-engine.sh` pins its zips by hash. The Engine workflow runs both on
GitHub's runners or on a self-hosted runner (repository variable
`ENGINE_RUNNER`); on any other x64 Linux machine run them by hand:

    ci/build-engine.sh ~/engine-work dist && ci/publish-engine.sh dist

Release layout, one zip per configuration, each with a `.sha256`:

| Asset | Contents |
| --- | --- |
| `linux-arm64-debug-embedder.zip` | `libflutter_engine.so` (JIT), `flutter_embedder.h`, `icudtl.dat`: the phone kit |
| `linux-arm64-profile-embedder.zip` | the same in profile (AOT), plus `gen_snapshot` (x64 host, arm64 target) |
| `linux-arm64-release-embedder.zip` | the same in release (AOT), plus `gen_snapshot` |
| `linux-x64-debug-embedder.zip` | debug engine for the simulator and the x64 (Cuttlefish) kit |

| Patch | What |
| --- | --- |
| 0001 | `shell/platform/embedder`: `FlutterWindowMetricsEvent` carries `physical_view_padding_{top,right,bottom,left}` (read through `struct_size`, so older embedders send zero) into `ViewportMetrics` `physical_padding_*`, the fields Android's embedding fills. Apps get `viewPadding` and `SafeArea` the stock way (Yuv: borrow what Android has) |
| 0002 | `build_overrides/vulkan_headers.gni`: no Vulkan XCB/Wayland WSI. From ardera/flutter-ci (MIT, Hannes Winkler), unchanged; we render offscreen |
| 0003 | `tools/gn`: no ANGLE X11/Wayland WSI for `--target-os linux` builds, which otherwise fail at gn gen. After ardera/flutter-ci's patch (MIT), adapted to this revision |

0001 is written to go upstream; 0002 and 0003 only make the minimal Linux
target build work.
