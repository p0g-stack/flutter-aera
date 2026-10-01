#!/usr/bin/env bash
# Text input through AERA's keyboard in the simulator: a tap focuses a
# TextField (KEYBOARD_SHOW, then the host's KEYBOARD_INSET), KEY messages
# type text with an accent and an emoji, Enter submits and hides the
# keyboard (KEYBOARD_HIDE, inset 0). The app prints what it saw.
# Run after ci/sim-smoke.sh WORKDIR (it fetches the engine).
#   ci/text-input-check.sh WORKDIR        (needs `flutter` 3.47.5 on PATH)
set -euo pipefail
work=$(realpath -m "${1:?usage: ci/text-input-check.sh WORKDIR}")
repo=$(cd "$(dirname "$0")/.." && pwd)
app=$work/text_input
if [ ! -d "$app" ]; then
  flutter create --no-pub --project-name text_input "$app" >/dev/null
fi
cp "$repo/ci/apps/text_input/main.dart" "$app/lib/main.dart"
(cd "$app" && flutter pub get >/dev/null && flutter build bundle --debug >/dev/null)
root=$work/payload-text
rm -rf "$root" && mkdir -p "$root/usr/lib" "$root/usr/share/flutter"
cp "$work/engine/embedder/libflutter_engine.so" "$root/usr/lib/"
cp "$work/engine/icudtl.dat" "$root/usr/share/flutter/"
cp -r "$app/build/flutter_assets" "$root/usr/share/flutter/"
log=$work/text-input.log
# The field sits under the app bar on the 1080x2400 x2.75 surface.
"$repo/target/release/aera-host-sim" --root "$root" --out "$work/text-input" --until 6000 \
  --tap 540,265@1500 --snap focused@2500 --type 'héllo 😀@3000' --snap typed@3800 \
  --type $'\n@4200' --snap done@5500 >"$log" 2>&1
check() { grep -qF "$1" "$log" || { echo "text input: missing '$1'" >&2; cat "$log" >&2; exit 1; }; }
check "sim: keyboard show (purpose 0, multiline 0)"
check "flutter: inset: 349"          # 960 px of keyboard / 2.75
check "flutter: submitted: héllo 😀"
check "sim: keyboard hide"
tail -n 1 <(grep "flutter: inset:" "$log") | grep -qF "inset: 0" || { echo "text input: inset not back to 0" >&2; exit 1; }
echo "text input: ok ($work/text-input)"
