#!/usr/bin/env bash
# Builds the default counter app for the pinned Flutter, stages an x64
# payload with the debug engine, runs it in aera-host-sim, taps "+" three
# times and checks the frames changed.
#   ci/sim-smoke.sh WORKDIR        (needs `flutter` 3.47.5 on PATH)
set -euo pipefail
work=$(realpath -m "${1:?usage: ci/sim-smoke.sh WORKDIR}")
repo=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$work"
"$repo/ci/fetch-engine.sh" "$work/engine"
if [ ! -d "$work/counter" ]; then
  flutter create --no-pub --project-name counter "$work/counter" >/dev/null
fi
(cd "$work/counter" && flutter pub get >/dev/null && flutter build bundle --debug >/dev/null)
root=$work/payload
rm -rf "$root" && mkdir -p "$root/usr/lib" "$root/usr/share/flutter"
cp "$work/engine/embedder/libflutter_engine.so" "$root/usr/lib/"
cp "$work/engine/icudtl.dat" "$root/usr/share/flutter/"
cp -r "$work/counter/build/flutter_assets" "$root/usr/share/flutter/"
cargo build --manifest-path "$repo/Cargo.toml" --workspace --release
# The FAB of the default app sits bottom-right; on the 1080x2400 x2.75
# phone surface its centre is about (980, 2250).
"$repo/target/release/aera-host-sim" --root "$root" --out "$work/sim" --until 4000 \
  --snap start@1500 --tap 980,2250@1700 --tap 980,2250@2000 --tap 980,2250@2300 --snap after@3500
if cmp -s "$work/sim/start.png" "$work/sim/after.png"; then
  echo "frames did not change after the taps" >&2
  exit 1
fi
echo "sim smoke: ok ($work/sim)"
