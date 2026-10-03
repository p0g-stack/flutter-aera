# Engine pin

| What | Pin |
| --- | --- |
| Flutter release | 3.47.5 (stable) |
| Framework / engine git revision | `af7e796e161ae0bb1ff0758c71a7105418bd9ded` (`bin/internal/engine.version`) |
| Engine content hash | `ab598368592da0064197e2bc15c7f5b0a2c6bb1f` (`engine_stamp.json`) |
| Dart SDK | 3.13.4 |
| `vendor/flutter/flutter_embedder.h` sha256 | `b9f20ec9453c623102da4d736076f57c0bb75a9e9beb50c196954a4899a01503` (stock header + `third_party/flutter-engine/patches`) |
| Engine release | `engine-af7e796-0d7f3ed2` (`ci/engine-tag.sh`: revision + sha256 of the patch series), built by `ci/build-engine.sh` |
| `linux-x64-debug-embedder.zip` sha256 (sim) | `3d5df7a667253dafb053b376ff44865518637d60aaf64ceb5130741aef20857b` |
| `linux-arm64-debug-embedder.zip` sha256 (phone, JIT) | `9f073512db3b971c0ea07657852897f03ffca7627fc0b48d54a4c9dd6d855c18` |
| `linux-arm64-profile-embedder.zip` sha256 (+ x64 `gen_snapshot`) | `d16e8cac4372e6096bbb7efa9e08102eb2e1c33f7bda87f6562e560571de94f3` |
| `linux-arm64-release-embedder.zip` sha256 (+ x64 `gen_snapshot`) | `6fd21fe01a29c04fbfe75e11d316e0e688ec7870ea551f2528bc6e69e1ed8197` |

Our engines, not Google's: built from the revision above with
`third_party/flutter-engine/patches` (0001 adds view padding) by
`ci/build-engine.sh` and published to
`https://github.com/p0g-stack/flutter-aera/releases/tag/engine-af7e796-0d7f3ed2`.
Each zip holds `libflutter_engine.so`, `flutter_embedder.h` and `icudtl.dat`.
`ci/fetch-engine.sh DIR ARCH [MODE]` fetches one, checks its hash, and checks
its header is `vendor/flutter/flutter_embedder.h` byte for byte. A Flutter
bump or a change to the patch series makes a new release name: run the
Engine workflow (or `ci/build-engine.sh` + `ci/publish-engine.sh` by hand),
then update the hashes here and in `ci/fetch-engine.sh`.

## Mesa

| What | Pin |
| --- | --- |
| Mesa | 26.2.2, `mesa-26.2.2.tar.xz` sha256 `eeb29ca7e56cfaa8e8a79538dcf834e3b18e501c31bef5145e959ea437cc4216` |
| Patch | `third_party/mesa/mesa-26.2.2-zink-kgsl-surfaceless.patch` (AERA Browser's) |
| Drivers | Zink + softpipe (gallium), Turnip on KGSL and msm (Vulkan) |

`tests/drift.rs` fails if the vendored header's hash is not in this file.
