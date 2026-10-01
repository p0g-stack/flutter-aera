# Engine pin

| What | Pin |
| --- | --- |
| Flutter release | 3.47.5 (stable) |
| Framework / engine git revision | `af7e796e161ae0bb1ff0758c71a7105418bd9ded` (`bin/internal/engine.version`) |
| Engine content hash | `ab598368592da0064197e2bc15c7f5b0a2c6bb1f` (`engine_stamp.json`) |
| Dart SDK | 3.13.4 |
| `vendor/flutter/flutter_embedder.h` sha256 | `94122469b254a932394bb5bb5c625317426eefd6e23d4366e32372c70ac5be37` |
| `linux-x64-embedder.zip` sha256 (sim engine, debug/JIT) | `d819c2a3aaa6c93bc57f4e535b6ad65500717f033b5576132da899b5a32fe86d` |
| `linux-arm64-embedder.zip` sha256 (phone engine, debug/JIT) | `2bfe19c80c007fc70731a4ff55d20436f95d8f565651d258714f8e274263fd47` |
| `linux-x64/artifacts.zip` sha256 (`icudtl.dat`, host `gen_snapshot`) | `b2e6e1e6b95866297b77580d04dbebcd45863585ad9f484108101c7c2f427a4a` |

Artifacts come from
`https://storage.googleapis.com/flutter_infra_release/flutter/<git revision>/linux-x64/<name>`
(the git revision, not the content hash, serves the embedder zip).
`ci/fetch-engine.sh` fetches them and checks these hashes.

The header is the one shipped inside both embedder zips (identical). The
runtime kit carries the arm64 debug engine; AOT release/profile engines will
be built from source by `ci/` and use the same header.

## Mesa

| What | Pin |
| --- | --- |
| Mesa | 26.2.2, `mesa-26.2.2.tar.xz` sha256 `eeb29ca7e56cfaa8e8a79538dcf834e3b18e501c31bef5145e959ea437cc4216` |
| Patch | `third_party/mesa/mesa-26.2.2-zink-kgsl-surfaceless.patch` (AERA Browser's) |
| Drivers | Zink + softpipe (gallium), Turnip on KGSL and msm (Vulkan) |

`tests/drift.rs` fails if the vendored header's hash is not in this file.
