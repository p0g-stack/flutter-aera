# Pins

Pins: what this repo holds fixed, where, and who moves it. Values read from the repo at the commit that added this file; the bump order across repos is /mnt/project-files/proposals/flutter-bump-checklist.md (project files). Engine and Mesa detail with hashes: `spec/engine-pin.md` (checked by `tests/drift.rs`).

| What | Where | Current | Bumped by |
| --- | --- | --- | --- |
| Flutter release / engine revision | `ci/fetch-engine.sh` `rev=`; `spec/engine-pin.md` | 3.47.5, `af7e796` | flutter-aera |
| Engine zips (Google debug, until ours land) | `ci/fetch-engine.sh` sha256 per zip | x64 `d819c2a3…`, arm64 `2bfe19c8…`, artifacts `b2e6e1e6…` | flutter-aera |
| Own engine release | name from `ci/engine-tag.sh` (rev + hash of `third_party/flutter-engine/patches`), built by `ci/build-engine.sh`, published by `ci/publish-engine.sh` / `engine.yml` | inferred: engine-af7e796-0d7f3ed2, not yet pinned by fetch-engine.sh | flutter-aera |
| Vendored embedder header | `vendor/flutter/flutter_embedder.h`, sha256 in `spec/engine-pin.md` | `b9f20ec9…` | flutter-aera, with the engine patches |
| AERA source the patch series applies to | `third_party/aera/README.md` `pin:` | `abf3316` (LVGL `017abcbf`) | flutter-aera |
| AERA protocol headers | `vendor/aera/protocol*.hpp`, sha256 in `spec/host.md` | `82d7b8df…`, `dd8a9740…` | flutter-aera, with the AERA pin |
| Mesa | `spec/engine-pin.md`, `ci/build-mesa.sh` | 26.2.2 + zink-kgsl patch | flutter-aera |
| Flutter in workflows | `kit.yml`, `mesa-debug.yml` `env.FLUTTER_VERSION` | 3.47.5 | flutter-aera |

Produces: `kit-<Flutter>` (Kit workflow, run by Yuv; re-uploads with `--clobber`, so consumers that want a fixed kit need its sha256) and `engine-<rev:7>-<series:8>` (Engine workflow).
