# Pins

Pins: what this repo holds fixed, where, and who moves it. Values read from the repo at the commit that added this file; the bump order across repos is https://github.com/p0g-stack/.github/blob/main/docs/flutter-bump.md. Engine and Mesa detail with hashes: `spec/engine-pin.md` (checked by `tests/drift.rs`).

| What | Where | Current | Bumped by |
| --- | --- | --- | --- |
| Flutter release / engine revision | `ci/fetch-engine.sh` `rev=`; `spec/engine-pin.md` | 3.47.9, `89c39ed` | flutter-aera |
| Engine zips (ours, release `engine-89c39ed-0d7f3ed2`) | `ci/fetch-engine.sh` sha256 per zip | x64-debug `3919ecd5…`, arm64-debug `6f8ebb3b…`, arm64-profile `840c93fe…`, arm64-release `1b7804fa…` | flutter-aera |
| Own engine release | name from `ci/engine-tag.sh` (rev + hash of `third_party/flutter-engine/patches`), built by `ci/build-engine.sh`, published by `ci/publish-engine.sh` / `engine.yml` | `engine-89c39ed-0d7f3ed2`, pinned by `ci/fetch-engine.sh` (`tag=` and the sha256s above) | flutter-aera |
| Vendored embedder header | `vendor/flutter/flutter_embedder.h`, sha256 in `spec/engine-pin.md` | `b9f20ec9…` | flutter-aera, with the engine patches |
| AERA source the patch series applies to | `third_party/aera/README.md` `pin:` | `9281346` (LVGL `212fd3a`) | flutter-aera |
| AERA protocol headers | `vendor/aera/protocol*.hpp`, sha256 in `spec/host.md` | `82d7b8df…`, `dd8a9740…` | flutter-aera, with the AERA pin |
| Mesa | `spec/engine-pin.md`, `ci/build-mesa.sh` | 26.2.2 + zink-kgsl patch | flutter-aera |
| Flutter in workflows | `kit.yml`, `mesa-debug.yml` `env.FLUTTER_VERSION` | 3.47.9 | flutter-aera |

Produces: `kit-<Flutter>-<flutter-aera full commit SHA>` (Kit workflow with `publish`, run by Yuv) and `engine-<rev:7>-<series:8>` (Engine workflow, `ci/publish-engine.sh`). Both upload through `.github/scripts/immutable-release-upload.sh`: a released asset is never replaced (identical bytes are skipped, different bytes fail the run), so changed kits come from a new commit and changed engines from a new patch series. The older `kit-<Flutter>` releases (e.g. `kit-3.47.5`) were re-uploaded with `--clobber`; they stay available but are not pinned by new consumers.
