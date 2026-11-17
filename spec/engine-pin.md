# Engine pin

| What | Pin |
| --- | --- |
| Flutter release | 3.47.9 (stable) |
| Framework git revision | `c197ca5199e9ee5661e865596c9c07294dfab51c` |
| Engine git revision | `89c39edfdfae2949f6806e4a315c0c08522238b7` (`bin/internal/engine.version`) |
| Engine content hash | `9e025265bef30b838c0c1b68cad19decf97048f2` (`engine_stamp.json`) |
| Dart SDK | 3.13.7 |
| `vendor/flutter/flutter_embedder.h` sha256 | `b9f20ec9453c623102da4d736076f57c0bb75a9e9beb50c196954a4899a01503` (exported by our patched engine build) |
| Engine release | `engine-89c39ed-0d7f3ed2` (`ci/engine-tag.sh`: revision + sha256 of the patch series), built by `ci/build-engine.sh` |
| `linux-x64-debug-embedder.zip` sha256 (sim) | `3919ecd5554e7c7bc06f86e26f799343ee55995ae17f6ff11a5fb1d628a8ccb4` |
| `linux-arm64-debug-embedder.zip` sha256 (phone, JIT) | `6f8ebb3b36f91080bfac8e9f382ddc1a33b78e28105467cf19bc09d19e675d6f` |
| `linux-arm64-profile-embedder.zip` sha256 (+ x64 `gen_snapshot`) | `840c93fe60f98c7c6fd5ad111e6a512092caf7c767008cf5eebac7f6142e8e60` |
| `linux-arm64-release-embedder.zip` sha256 (+ x64 `gen_snapshot`) | `1b7804fa484581e36adf7c83c64443b4b815676fbe3759d0886ca6a8845b5db5` |

Our engines, not Google's: built from the revision above with
`third_party/flutter-engine/patches` (0001 adds view padding) by
`ci/build-engine.sh` and published to
`https://github.com/p0g-stack/flutter-aera/releases/tag/engine-89c39ed-0d7f3ed2`.
Each zip holds `libflutter_engine.so`, `flutter_embedder.h` and `icudtl.dat`.
`ci/fetch-engine.sh DIR ARCH [MODE]` fetches one, checks its hash, and checks
its header is `vendor/flutter/flutter_embedder.h` byte for byte. A Flutter
bump or a change to the patch series makes a new release name: run the
Engine workflow (or `ci/build-engine.sh` + `ci/publish-engine.sh` by hand),
then update the hashes here and in `ci/fetch-engine.sh`.

Flutter 3.47.9 production: [Engine run 44038868645](https://github.com/p0g-stack/flutter-aera/actions/runs/44038868645),
producer `1234b0e4f0862261dd430dc05d89fd5aa3b49e05`. The existing patch series
applies unchanged; the patched source tree is
`f63c1114ee77433428e85ea43c567034d7c1a093`. All four built headers are
byte-identical to the vendored header; no ABI update is needed.

## Mesa

| What | Pin |
| --- | --- |
| Mesa | 26.2.2, `mesa-26.2.2.tar.xz` sha256 `eeb29ca7e56cfaa8e8a79538dcf834e3b18e501c31bef5145e959ea437cc4216` |
| Patch | `third_party/mesa/mesa-26.2.2-zink-kgsl-surfaceless.patch` (AERA Browser's) |
| Drivers | Zink + softpipe (gallium), Turnip on KGSL and msm (Vulkan) |

`tests/drift.rs` fails if the vendored header's hash is not in this file.
