#!/usr/bin/env bash
# Builds the pinned Mesa (spec/engine-pin.md) natively: EGL + GLES through
# Zink on Turnip (KGSL, arm64 only), virgl for virtio-gpu (Cuttlefish) and
# softpipe for the simulator. Installs into DEST/usr.
#   ci/build-mesa.sh DEST
set -euo pipefail
dest=$(realpath -m "${1:?usage: ci/build-mesa.sh DEST}")
repo=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
sha=eeb29ca7e56cfaa8e8a79538dcf834e3b18e501c31bef5145e959ea437cc4216
curl -fsSL -o "$work/mesa.tar.xz" https://archive.mesa3d.org/mesa-26.2.2.tar.xz
echo "$sha  $work/mesa.tar.xz" | sha256sum -c -
tar -C "$work" -xf "$work/mesa.tar.xz"
cd "$work/mesa-26.2.2"
case $(uname -m) in
  aarch64) vulkan=(-Dvulkan-drivers=freedreno -Dfreedreno-kmds=msm,kgsl) ;;
  *) vulkan=(-Dvulkan-drivers=) ;;
esac
patch -p1 < "$repo/third_party/mesa/mesa-26.2.2-zink-kgsl-surfaceless.patch"
meson setup build --wrap-mode=nodownload --prefix=/usr --libdir=lib \
  -Dbuildtype=release -Db_ndebug=true -Dplatforms= -Degl=enabled -Dgles1=disabled \
  -Dgles2=enabled -Dopengl=true -Dglx=disabled -Dgbm=disabled -Dglvnd=disabled \
  -Dgallium-drivers=zink,softpipe,virgl "${vulkan[@]}" \
  -Dllvm=disabled -Dvalgrind=disabled -Dlibunwind=disabled -Dlmsensors=disabled \
  -Dbuild-tests=false -Dvideo-codecs= -Dvulkan-layers= -Dtools= -Dzstd=enabled \
  -Dexpat=enabled -Dteflon=false
ninja -C build
DESTDIR="$dest" ninja -C build install
