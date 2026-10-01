#!/usr/bin/env bash
# Fetches the pinned linux-x64 debug embedder engine and host artifacts
# (spec/engine-pin.md) into DIR and checks their hashes.
#   ci/fetch-engine.sh DIR
set -euo pipefail
dir=${1:?usage: ci/fetch-engine.sh DIR}
rev=af7e796e161ae0bb1ff0758c71a7105418bd9ded
base=https://storage.googleapis.com/flutter_infra_release/flutter/$rev/linux-x64
mkdir -p "$dir"
fetch() {
  curl -fsSL -o "$dir/$1" "$base/$1"
  echo "$2  $dir/$1" | sha256sum -c -
}
fetch linux-x64-embedder.zip d819c2a3aaa6c93bc57f4e535b6ad65500717f033b5576132da899b5a32fe86d
fetch artifacts.zip b2e6e1e6b95866297b77580d04dbebcd45863585ad9f484108101c7c2f427a4a
unzip -o -q "$dir/linux-x64-embedder.zip" -d "$dir/embedder"
unzip -o -q "$dir/artifacts.zip" icudtl.dat -d "$dir"
cmp "$dir/embedder/flutter_embedder.h" "$(dirname "$0")/../vendor/flutter/flutter_embedder.h"
