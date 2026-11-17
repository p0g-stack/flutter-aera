#!/usr/bin/env bash
# Host-only end-to-end proof. FLUTTER must match the branch's pinned engine.
# Usage: FLUTTER=/path/to/flutter ci/api2-check.sh WORKDIR
# No device listing/selection, adb, flashing, mounting, or unlocking.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
work=$(realpath -m "${1:?usage: ci/api2-check.sh WORKDIR}")
flutter=${FLUTTER:-flutter}
mkdir -p "$work"
if [[ ! -f "$work/engine/embedder/libflutter_engine.so" ]]; then
  "$repo/ci/fetch-engine.sh" "$work/engine" x64 debug
fi
run=$(mktemp -d "$work/run.XXXXXX")
app=$run/app
mkdir -p "$app/lib"
cp "$repo/ci/apps/api2_probe/pubspec.yaml" "$app/pubspec.yaml"
cp "$repo/ci/apps/api2_probe/main.dart" "$app/lib/main.dart"
(cd "$app" && "$flutter" pub get && "$flutter" build bundle --debug)
root=$run/payload
mkdir -p "$root/usr/lib" "$root/usr/share/flutter"
cp "$work/engine/embedder/libflutter_engine.so" "$root/usr/lib/"
cp "$work/engine/icudtl.dat" "$root/usr/share/flutter/"
cp -a "$app/build/flutter_assets" "$root/usr/share/flutter/"
printf 'API 2 selected file fixture\n' > "$run/selected.txt"
(cd "$repo" && cargo test --workspace && cargo build --workspace)
sim=$repo/target/debug/aera-host-sim
"$sim" --base-api2 --root "$root" --out "$run/base" --until 3500 \
  --snap before@700 --tap 916,496@1000 --tap 916,496@1400 \
  --snap after@2000 --tap 70,333@2200 --back@2700 > "$run/base.log" 2>&1
"$sim" --root "$root" --out "$run/extended" --pick-file "$run/selected.txt" --until 6500 \
  --tap 400,170@900 --snap focused@1500 --type 'héllo 😀@1700' \
  --snap typed@2200 --type $'\n@2500' --snap submitted@3000 \
  --tap 80,280@3500 --snap picked@4200 --tap 70,333@4600 \
  --back@5100 --snap returned@5800 > "$run/extended.log" 2>&1
check() { grep -qF "$2" "$1" || { cat "$1"; echo "missing: $2" >&2; exit 1; }; }
check "$run/base.log" 'api2: count: 2'
check "$run/base.log" 'api2: back returned'
if cmp -s "$run/base/before.png" "$run/base/after.png"; then
  echo 'counter screenshots are identical' >&2; exit 1
fi
check "$run/extended.log" 'api2: submitted: héllo 😀'
check "$run/extended.log" "api2: picked: $run/selected.txt"
check "$run/extended.log" 'api2: back returned'
grep -E 'api2: inset: [1-9][0-9]*' "$run/extended.log"
tail -n 1 <(grep 'api2: inset:' "$run/extended.log") | grep -qF 'api2: inset: 0'
printf 'API 2 proof passed: %s\n' "$run"
