#!/usr/bin/env bash
# Prints the release that holds the engines for the pinned revision and the
# current third_party/flutter-engine/patches: engine-<rev:7>-<series:8>.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
rev=$(sed -n 's/^rev=//p' "$repo/ci/fetch-engine.sh")
patches=$repo/third_party/flutter-engine/patches
echo "engine-${rev:0:7}-$(sed "s|^|$patches/|" "$patches/series" | xargs cat | sha256sum | cut -c1-8)"
