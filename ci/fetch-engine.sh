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
rev=af7e796e161ae0bb1ff0758c71a7105418bd9ded
tag=engine-af7e796-0d7f3ed2
base=https://github.com/p0g-stack/flutter-aera/releases/download/$tag
case $arch-$mode in
  arm64-debug) sum=9f073512db3b971c0ea07657852897f03ffca7627fc0b48d54a4c9dd6d855c18 ;;
  arm64-profile) sum=d16e8cac4372e6096bbb7efa9e08102eb2e1c33f7bda87f6562e560571de94f3 ;;
  arm64-release) sum=6fd21fe01a29c04fbfe75e11d316e0e688ec7870ea551f2528bc6e69e1ed8197 ;;
  x64-debug) sum=3d5df7a667253dafb053b376ff44865518637d60aaf64ceb5130741aef20857b ;;
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
