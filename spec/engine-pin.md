# Engine pin

| What | Pin |
| --- | --- |
| Flutter release | 3.47.5 (stable) |
| Framework / engine git revision | `af7e796e161ae0bb1ff0758c71a7105418bd9ded` (`bin/internal/engine.version`) |
| Engine content hash | `ab598368592da0064197e2bc15c7f5b0a2c6bb1f` (`engine_stamp.json`) |
| Dart SDK | 3.13.4 |
| `vendor/flutter/flutter_embedder.h` sha256 | `94122469b254a932394bb5bb5c625317426eefd6e23d4366e32372c70ac5be37` |
| `linux-x64-embedder.zip` sha256 (sim engine, debug/JIT) | `d819c2a3aaa6c93bc57f4e535b6ad65500717f033b5576132da899b5a32fe86d` |
| `linux-x64/artifacts.zip` sha256 (`icudtl.dat`, host `gen_snapshot`) | `b2e6e1e6b95866297b77580d04dbebcd45863585ad9f484108101c7c2f427a4a` |

Artifacts come from
`https://storage.googleapis.com/flutter_infra_release/flutter/<git revision>/linux-x64/<name>`
(the git revision, not the content hash, serves the embedder zip).
`ci/fetch-engine.sh` fetches them and checks these hashes.

The header is the one shipped inside `linux-x64-embedder.zip`. The arm64
engine for the phone (`linux-arm64/linux-arm64-embedder.zip` for debug; AOT
release/profile built from source by `ci/`) uses the same header.

`tests/drift.rs` fails if the vendored header's hash is not in this file.
