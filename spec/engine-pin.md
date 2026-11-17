# Engine pin

| What | Pin |
| --- | --- |
| Flutter release | 3.50.0 (stable) |
| Framework git revision | `ab2912035e160d9cad6306df307576da6128ebdb` |
| Engine git revision | `371f77202a73e8dff57a8b3bff75bca55c090aee` (`bin/internal/engine.version`) |
| Engine content hash | `bf601093ad255aaf5c6d0fc5c4f9acfa60943b2a` (`engine_stamp.json`) |
| Dart SDK | 3.14.0 |
| `vendor/flutter/flutter_embedder.h` sha256 | `91745745a9a60b01a3255260722677e1fe17a832064b040ff5d1d2389a974994` (exported by our patched engine build) |
| Engine release | `engine-371f772-0d7f3ed2` (`ci/engine-tag.sh`: revision + sha256 of the patch series), built by `ci/build-engine.sh` |
| `linux-x64-debug-embedder.zip` sha256 (sim) | `df9099fc821e9b138a2536ff7ac9fdfcd6612eaecd301166ca9bad7de8374664` |
| `linux-arm64-debug-embedder.zip` sha256 (phone, JIT) | `77abf5a14a97cd9e1a084cf8bfe8f532de2ba6020a994e69b4ea968648496b4b` |
| `linux-arm64-profile-embedder.zip` sha256 (+ x64 `gen_snapshot`) | `b4306275a327c5aeb4216cbffa6688ce4a9cb0d17f5aba53888ca4591890463d` |
| `linux-arm64-release-embedder.zip` sha256 (+ x64 `gen_snapshot`) | `b96d1248ce56fb8c5ac154901934710072cad79e642279e67bc746a36208419e` |

Our engines, not Google's: built from the revision above with
`third_party/flutter-engine/patches` (0001 adds view padding) by
`ci/build-engine.sh` and published to
`https://github.com/p0g-stack/flutter-aera/releases/tag/engine-371f772-0d7f3ed2`.
Each zip holds `libflutter_engine.so`, `flutter_embedder.h` and `icudtl.dat`.
`ci/fetch-engine.sh DIR ARCH [MODE]` fetches one, checks its hash, and checks
its header is `vendor/flutter/flutter_embedder.h` byte for byte. A Flutter
bump or a change to the patch series makes a new release name: run the
Engine workflow (or `ci/build-engine.sh` + `ci/publish-engine.sh` by hand),
then update the hashes here and in `ci/fetch-engine.sh`.

Flutter 3.50.0 production: [Engine run 44125535399](https://github.com/p0g-stack/flutter-aera/actions/runs/44125535399),
producer `dba6fa7c817311efc315a881105762551dbf1fe0`. The existing patch series
applies unchanged; the patched source tree is
`d23d5f801a554e5b9936497b8e7cfd36a65cda18`. The upstream header adds Vulkan
external-texture declarations and frozen-ABI comments; the renderer's zeroed
configuration leaves the new optional callback unset.
All four published headers match the vendored header byte for byte.

## Mesa

| What | Pin |
| --- | --- |
| Mesa | 26.2.7, `mesa-26.2.7.tar.xz` sha256 `04294f75d22e9f61cd1e050206bc48b3d39366e788f1803a3368e3d5a872ce18` |
| Patch | `third_party/mesa/mesa-26.2.7-zink-kgsl-surfaceless.patch` (AERA Browser's) |
| Drivers | Zink + softpipe (gallium), Turnip on KGSL and msm (Vulkan) |

`tests/drift.rs` fails if the vendored header's hash is not in this file.
