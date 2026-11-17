#!/usr/bin/env bash
# Uploads the engines ci/build-engine.sh left in DIST to the release named by
# ci/engine-tag.sh, creating it if needed. Assets are immutable
# (.github/scripts/immutable-release-upload.sh): one already released with
# identical bytes is skipped, one with different bytes fails the run and is
# left in place; changed engines need a new patch series, hence a new tag.
# The Engine workflow runs this; after a build on another machine, run it
# there with `gh` signed in.
#   ci/publish-engine.sh DIST
set -euo pipefail
dist=${1:?usage: ci/publish-engine.sh DIST}
repo=$(cd "$(dirname "$0")/.." && pwd)
rev=$(sed -n 's/^rev=//p' "$repo/ci/fetch-engine.sh")
tag=$("$repo/ci/engine-tag.sh")
"$repo/.github/scripts/immutable-release-upload.sh" -R p0g-stack/flutter-aera "$tag" "$dist"/*.zip "$dist"/*.sha256 -- \
  --target main --prerelease \
  --title "Flutter engine ${rev:0:7}, flutter-aera patches" \
  --notes "Embedder engines built from flutter/flutter $rev with third_party/flutter-engine/patches by ci/build-engine.sh. One zip per architecture and mode; arm64 profile and release carry an x64-hosted gen_snapshot. Pinned by ci/fetch-engine.sh (spec/engine-pin.md)."
echo "$tag"
