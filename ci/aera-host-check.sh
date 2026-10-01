#!/usr/bin/env bash
# Applies third_party/aera/patches to AERA at its pin, builds AERA's Host API
# 3 tests and runs the counter through AERA's own launcher, session and pixel
# surface (no LVGL). Run after ci/sim-smoke.sh WORKDIR, which builds the
# counter's assets and the embedder.
#   sudo ci/aera-host-check.sh WORKDIR   (root: AERA's launcher insists)
set -euo pipefail
work=$(realpath -m "${1:?usage: ci/aera-host-check.sh WORKDIR}")
repo=$(cd "$(dirname "$0")/.." && pwd)
pin=$(sed -n 's/^pin: *//p' "$repo/third_party/aera/README.md")
kit=https://github.com/p0g-stack/flutter-aera/releases/download/kit-3.47.5/flutter-aera-kit-linux-x64-debug-3.47.5.tar.xz

aera=$work/aera
if [ ! -d "$aera/.git" ]; then
  git init -q "$aera"
  git -C "$aera" remote add origin https://github.com/AERA-Recovery/android_bootable_recovery
  git -C "$aera" sparse-checkout set aeraui
fi
git -C "$aera" fetch -q --depth 1 origin "$pin"
git -C "$aera" checkout -q --detach FETCH_HEAD
git -C "$aera" -c user.name=ci -c user.email=ci@localhost am -q \
  $(sed "s|^|$repo/third_party/aera/patches/|" "$repo/third_party/aera/patches/series")

# i18n.hpp includes lvgl.h; nothing from LVGL is linked.
[ -d "$work/lvgl" ] || git clone -q --depth 1 -b release/v9.3 https://github.com/lvgl/lvgl "$work/lvgl"

ui=$aera/aeraui
cxx="c++ -std=c++17 -UNDEBUG -DLV_CONF_SKIP -fsanitize=address,undefined -I$ui -I$ui/features -I$ui/include -I$work/lvgl"
$cxx "$ui/tests/plugin_api_session_test.cpp" "$ui/features/plugin_api/session.cpp" -o "$work/session_test"
$cxx "$ui/tests/plugin_api_surface_test.cpp" "$ui/features/plugin_api/surface.cpp" -o "$work/surface_test"
$cxx "$ui/tests/plugin_api_pixel_host_check.cpp" "$ui/features/plugin_api/"{launcher,session,surface}.cpp \
  -o "$work/pixel_host_check"
"$work/session_test"
"$work/surface_test"

# The published kit with this commit's embedder and the counter's assets.
tree=$work/runtime
rm -rf "$tree" && mkdir -p "$tree"
curl -fsSL "$kit" -o "$work/kit.tar.xz"
echo "$(curl -fsSL "$kit.sha256" | cut -d' ' -f1)  $work/kit.tar.xz" | sha256sum -c -
tar -xJf "$work/kit.tar.xz" -C "$tree" --no-same-owner
rm "$tree/kit.json"
install -m 0755 "$repo/target/release/aera-flutter" "$tree/usr/bin/aera-flutter"
cp -r "$work/counter/build/flutter_assets" "$tree/usr/share/flutter/"
chown -R 0:0 "$tree"

mkdir -p "$work/aera-host"
"$work/pixel_host_check" "$tree" "$work/aera-host"
echo "aera host check: ok ($work/aera-host)"
