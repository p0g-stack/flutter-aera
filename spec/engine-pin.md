# Engine pin

| What | Pin |
| --- | --- |
| Flutter release | 3.47.5 (stable) |
| Framework / engine git revision | `af7e796e161ae0bb1ff0758c71a7105418bd9ded` (`bin/internal/engine.version`) |
| Engine content hash | `ab598368592da0064197e2bc15c7f5b0a2c6bb1f` (`engine_stamp.json`) |
| Dart SDK | 3.13.4 |
| `vendor/flutter/flutter_embedder.h` sha256 | `b9f20ec9453c623102da4d736076f57c0bb75a9e9beb50c196954a4899a01503` (stock header + `third_party/flutter-engine/patches`) |
| `linux-x64-embedder.zip` sha256 (sim engine, debug/JIT) | `d819c2a3aaa6c93bc57f4e535b6ad65500717f033b5576132da899b5a32fe86d` |
| `linux-arm64-embedder.zip` sha256 (phone engine, debug/JIT) | `2bfe19c80c007fc70731a4ff55d20436f95d8f565651d258714f8e274263fd47` |
| `linux-x64/artifacts.zip` sha256 (`icudtl.dat`, host `gen_snapshot`) | `b2e6e1e6b95866297b77580d04dbebcd45863585ad9f484108101c7c2f427a4a` |

Artifacts come from
`https://storage.googleapis.com/flutter_infra_release/flutter/<git revision>/linux-x64/<name>`
(the git revision, not the content hash, serves the embedder zip).
`ci/fetch-engine.sh` fetches them and checks these hashes.

The vendored header is Google's plus `third_party/flutter-engine/patches`
(0001 adds view padding); `ci/fetch-engine.sh` checks the stock header with
the patches applied. Our own engines (debug, profile and release, built from
source by `ci/build-engine.sh`, release `engine-<revision>-<series hash>`)
replace Google's zips here once published; until then the kit runs Google's
debug engine, which ignores the padding fields.

## Mesa

| What | Pin |
| --- | --- |
| Mesa | 26.2.2, `mesa-26.2.2.tar.xz` sha256 `eeb29ca7e56cfaa8e8a79538dcf834e3b18e501c31bef5145e959ea437cc4216` |
| Patch | `third_party/mesa/mesa-26.2.2-zink-kgsl-surfaceless.patch` (AERA Browser's) |
| Drivers | Zink + softpipe (gallium), Turnip on KGSL and msm (Vulkan) |

`tests/drift.rs` fails if the vendored header's hash is not in this file.
