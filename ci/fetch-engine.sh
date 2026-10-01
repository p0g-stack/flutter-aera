#!/usr/bin/env bash
# Fetches the pinned debug (JIT) embedder engine for ARCH and ICU data
# (spec/engine-pin.md) into DIR and checks their hashes.
#   ci/fetch-engine.sh DIR [x64|arm64]
set -euo pipefail
dir=${1:?usage: ci/fetch-engine.sh DIR [x64|arm64]}
arch=${2:-x64}
rev=af7e796e161ae0bb1ff0758c71a7105418bd9ded
base=https://storage.googleapis.com/flutter_infra_release/flutter/$rev
mkdir -p "$dir"
fetch() {
  curl -fsSL -o "$dir/$(basename "$1")" "$base/$1"
  echo "$2  $dir/$(basename "$1")" | sha256sum -c -
}
case $arch in
  x64) fetch linux-x64/linux-x64-embedder.zip d819c2a3aaa6c93bc57f4e535b6ad65500717f033b5576132da899b5a32fe86d ;;
  arm64) fetch linux-arm64/linux-arm64-embedder.zip 2bfe19c80c007fc70731a4ff55d20436f95d8f565651d258714f8e274263fd47 ;;
  *) echo "unknown arch $arch" >&2; exit 2 ;;
esac
# ICU data is architecture independent; the x64 host artifacts carry it.
fetch linux-x64/artifacts.zip b2e6e1e6b95866297b77580d04dbebcd45863585ad9f484108101c7c2f427a4a
rm -rf "$dir/embedder"
unzip -o -q "$dir/linux-$arch-embedder.zip" -d "$dir/embedder"
unzip -o -q "$dir/artifacts.zip" icudtl.dat -d "$dir"
cmp "$dir/embedder/flutter_embedder.h" "$(dirname "$0")/../vendor/flutter/flutter_embedder.h"
