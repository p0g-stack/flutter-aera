#!/usr/bin/env bash
# Uploads the engines ci/build-engine.sh left in DIST to the release named by
# ci/engine-tag.sh, creating it if needed. The Engine workflow runs this;
# after a build on another machine, run it there with `gh` signed in.
#   ci/publish-engine.sh DIST
set -euo pipefail
dist=${1:?usage: ci/publish-engine.sh DIST}
repo=$(cd "$(dirname "$0")/.." && pwd)
rev=$(sed -n 's/^rev=//p' "$repo/ci/fetch-engine.sh")
tag=$("$repo/ci/engine-tag.sh")
gh release view "$tag" -R p0g-stack/flutter-aera >/dev/null 2>&1 ||
  gh release create "$tag" -R p0g-stack/flutter-aera --target main --prerelease \
    --title "Flutter engine ${rev:0:7}, flutter-aera patches" \
    --notes "Embedder engines built from flutter/flutter $rev with third_party/flutter-engine/patches by ci/build-engine.sh. One zip per architecture and mode; arm64 profile and release carry an x64-hosted gen_snapshot. Pinned by ci/fetch-engine.sh (spec/engine-pin.md)."
gh release upload "$tag" -R p0g-stack/flutter-aera "$dist"/*.zip "$dist"/*.sha256 --clobber
echo "$tag"
