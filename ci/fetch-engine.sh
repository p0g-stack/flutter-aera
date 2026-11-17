#!/usr/bin/env bash
# Fetches our pinned embedder engine for ARCH and MODE (spec/engine-pin.md),
# built by ci/build-engine.sh from the revision below with
# third_party/flutter-engine/patches, into DIR and checks its hash. Leaves
# DIR/embedder/{libflutter_engine.so,flutter_embedder.h[,gen_snapshot]} and
# DIR/icudtl.dat.
#   ci/fetch-engine.sh DIR [x64|arm64] [debug|profile|release]
set -euo pipefail
dir=${1:?usage: ci/fetch-engine.sh DIR [x64|arm64] [debug|profile|release]}
arch=${2:-x64}
mode=${3:-debug}
rev=89c39edfdfae2949f6806e4a315c0c08522238b7
tag=engine-89c39ed-0d7f3ed2
base=https://github.com/p0g-stack/flutter-aera/releases/download/$tag
case $arch-$mode in
  arm64-debug) sum=6f8ebb3b36f91080bfac8e9f382ddc1a33b78e28105467cf19bc09d19e675d6f ;;
  arm64-profile) sum=840c93fe60f98c7c6fd5ad111e6a512092caf7c767008cf5eebac7f6142e8e60 ;;
  arm64-release) sum=1b7804fa484581e36adf7c83c64443b4b815676fbe3759d0886ca6a8845b5db5 ;;
  x64-debug) sum=3919ecd5554e7c7bc06f86e26f799343ee55995ae17f6ff11a5fb1d628a8ccb4 ;;
  *) echo "no engine for $arch $mode" >&2; exit 2 ;;
esac
zip=linux-$arch-$mode-embedder.zip
mkdir -p "$dir"
curl -fsSL -o "$dir/$zip" "$base/$zip"
echo "$sum  $dir/$zip" | sha256sum -c -
rm -rf "$dir/embedder"
unzip -o -q "$dir/$zip" -d "$dir/embedder"
mv "$dir/embedder/icudtl.dat" "$dir/icudtl.dat"
# The header the engine was built with is the one we bind against.
repo=$(cd "$(dirname "$0")/.." && pwd)
cmp "$dir/embedder/flutter_embedder.h" "$repo/vendor/flutter/flutter_embedder.h"
