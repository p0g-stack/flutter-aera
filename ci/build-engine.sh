#!/usr/bin/env bash
# Builds Flutter's embedder engine from the pinned revision with
# third_party/flutter-engine/patches, the way flutter-pi's engine builds do
# (ardera/flutter-ci): embedder only, minimal Linux, no desktop shells, no
# Android/Fuchsia/Linux-desktop dependencies. Runs on any x64 Linux host: a
# GitHub runner, a self-hosted runner or Yuv's build machine (needs git,
# python3, curl, about 40 GB free and a few hours per configuration).
#
#   ci/build-engine.sh WORKDIR DIST [CONFIG...]
#
# CONFIG is ARCH-MODE: arm64-debug, arm64-profile, arm64-release, x64-debug
# (default: all four). WORKDIR keeps depot_tools and the source between runs.
# DIST receives, per configuration, linux-<arch>-<mode>-embedder.zip with
# libflutter_engine.so, flutter_embedder.h and icudtl.dat, plus gen_snapshot
# (x64 host, arm64 target) for arm64 profile and release, which AOT-compiles
# the app (flutter_p0g's host/gen_snapshot); and a .sha256 per zip. The
# release they go into is named by ci/engine-tag.sh.
set -euo pipefail
work=$(realpath -m "${1:?usage: ci/build-engine.sh WORKDIR DIST [CONFIG...]}")
dist=$(realpath -m "${2:?usage: ci/build-engine.sh WORKDIR DIST [CONFIG...]}")
shift 2
configs=("$@")
[ ${#configs[@]} -gt 0 ] || configs=(arm64-debug arm64-profile arm64-release x64-debug)
repo=$(cd "$(dirname "$0")/.." && pwd)
rev=$(sed -n 's/^rev=//p' "$repo/ci/fetch-engine.sh")
patches=$repo/third_party/flutter-engine/patches
mkdir -p "$work" "$dist"

if ! command -v gclient >/dev/null; then
  [ -d "$work/depot_tools" ] || git clone -q --depth 1 https://chromium.googlesource.com/chromium/tools/depot_tools.git "$work/depot_tools"
  export PATH=$work/depot_tools:$PATH
fi
export DEPOT_TOOLS_UPDATE=0

# Source: flutter/flutter at the pin, our patches on top, then its DEPS.
src=$work/flutter
stamp="$rev $(cat "$patches/series" | sed "s|^|$patches/|" | xargs cat | sha256sum | cut -c1-16)"
if [ "$(cat "$work/source.stamp" 2>/dev/null)" != "$stamp" ]; then
  rm -f "$work/source.stamp"
  if [ ! -d "$src/.git" ]; then
    git init -q "$src"
    git -C "$src" remote add origin https://github.com/flutter/flutter.git
  fi
  git -C "$src" fetch -q --depth 1 origin "$rev"
  git -C "$src" checkout -q -f --detach FETCH_HEAD
  git -C "$src" -c user.name=engine -c user.email=engine@localhost am -q \
    $(sed "s|^|$patches/|" "$patches/series")
  cat > "$src/.gclient" <<'EOF'
solutions = [{
  "managed": False,
  "name": ".",
  "url": "https://github.com/flutter/flutter.git",
  "deps_file": "DEPS",
  "custom_vars": {
    "download_linux_deps": False,
    "download_android_deps": False,
    "download_jdk": False,
    "download_esbuild": False,
    "download_fuchsia_deps": False,
  },
}]
EOF
  (cd "$src" && gclient sync -D --no-history --shallow)
  echo "$stamp" > "$work/source.stamp"
fi

# The Debian sysroots the build links against (download_linux_deps is off,
# so gclient does not fetch them); x64 also hosts gen_snapshot. Idempotent.
for a in amd64 arm64; do
  "$src/engine/src/build/linux/sysroot_scripts/install-sysroot.py" --arch=$a
done

for config in "${configs[@]}"; do
  arch=${config%-*}
  mode=${config#*-}
  case $arch in
    x64) cpu=() ;;
    arm64) cpu=(--target-os linux --linux-cpu arm64) ;;
    *) echo "unknown arch in $config" >&2; exit 2 ;;
  esac
  case $mode in debug|profile|release) ;; *) echo "unknown mode in $config" >&2; exit 2 ;; esac
  out=aera_${arch}_$mode
  (
    cd "$src/engine/src"
    ./flutter/tools/gn --runtime-mode "$mode" "${cpu[@]}" --target-dir "$out" \
      --embedder-for-target --enable-minimal-linux --disable-desktop-embeddings \
      --no-build-glfw-shell --no-build-embedder-examples --no-enable-unittests \
      --no-goma --no-rbe
    targets=(libflutter_engine.so flutter_embedder.h icudtl.dat)
    if [ "$arch" = arm64 ] && [ "$mode" != debug ]; then targets+=(clang_x64/gen_snapshot); fi
    # The ninja Flutter's DEPS pins (depot_tools' wrapper wants a bootstrap
    # we skip).
    "$src/third_party/ninja/ninja" -C "out/$out" "${targets[@]}"
  )
  stage=$work/stage/$config
  rm -rf "$stage" && mkdir -p "$stage"
  o=$src/engine/src/out/$out
  cp "$o/libflutter_engine.so" "$o/flutter_embedder.h" "$o/icudtl.dat" "$stage/"
  if [ -f "$o/clang_x64/gen_snapshot" ] && [ "$arch" = arm64 ] && [ "$mode" != debug ]; then
    cp "$o/clang_x64/gen_snapshot" "$stage/"
  fi
  zip=linux-$arch-$mode-embedder.zip
  rm -f "$dist/$zip"
  (cd "$stage" && zip -q -X -r "$dist/$zip" .)
  (cd "$dist" && sha256sum "$zip" > "$zip.sha256" && cat "$zip.sha256")
done
echo "release: $("$repo/ci/engine-tag.sh")"
