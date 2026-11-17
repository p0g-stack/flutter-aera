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
rev=371f77202a73e8dff57a8b3bff75bca55c090aee
tag=engine-371f772-0d7f3ed2
base=https://github.com/p0g-stack/flutter-aera/releases/download/$tag
case $arch-$mode in
  arm64-debug) sum=77abf5a14a97cd9e1a084cf8bfe8f532de2ba6020a994e69b4ea968648496b4b ;;
  arm64-profile) sum=b4306275a327c5aeb4216cbffa6688ce4a9cb0d17f5aba53888ca4591890463d ;;
  arm64-release) sum=b96d1248ce56fb8c5ac154901934710072cad79e642279e67bc746a36208419e ;;
  x64-debug) sum=df9099fc821e9b138a2536ff7ac9fdfcd6612eaecd301166ca9bad7de8374664 ;;
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
