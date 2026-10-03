#!/usr/bin/env bash
# The counter under each renderer: GL (default) and Vulkan (--vulkan), each
# with Skia and with Impeller (--enable-impeller). Each run taps "+" three
# times; passes when the log names the expected backend, the app bar is at
# the top (frames are upright) and the taps changed the frame.
# Run after ci/sim-smoke.sh WORKDIR. Vulkan needs a driver (lavapipe in CI).
#   ci/renderer-check.sh WORKDIR
set -euo pipefail
work=$(realpath -m "${1:?usage: ci/renderer-check.sh WORKDIR}")
repo=$(cd "$(dirname "$0")/.." && pwd)
run() {
  local name=$1 expect=$2; shift 2
  local out=$work/renderer-$name
  rm -rf "$out"
  local env=(FLUTTER_ENGINE_SWITCHES=$#)
  local i=1
  for s in "$@"; do env+=("FLUTTER_ENGINE_SWITCH_$i=$s"); i=$((i + 1)); done
  env "${env[@]}" "$repo/target/release/aera-host-sim" --root "$work/payload" --out "$out" --until 4000 \
    --snap start@1500 --tap 980,2250@1700 --tap 980,2250@2000 --tap 980,2250@2300 --snap after@3500 \
    >"$out.log" 2>&1
  if ! grep -q "$expect" "$out.log"; then
    echo "renderer $name: no '$expect' in the log" >&2; cat "$out.log" >&2; exit 1
  fi
  python3 "$repo/ci/frame_check.py" "$out/start.png" "$out/after.png"
  echo "renderer $name: ok"
}
run gl "GL renderer"
run gl-impeller "Impeller rendering backend (OpenGLES)" --enable-impeller
run vulkan "Vulkan renderer" --vulkan
run vulkan-impeller "Impeller rendering backend (Vulkan)" --vulkan --enable-impeller

# Rotations, as on a 720x1348 phone with a 165-row status bar: the surface
# turns from 720x1183 to 1348x555, wider than the first shape's longer side,
# and back. The app must keep drawing (devicelab D12: Vulkan kept the old
# shape), and the first frame of each shape must be upright and whole: it is
# the one frame the counter draws after a turn (Infiniti: with the flip about
# the old height it was drawn off the surface, and later partial repaints
# left the rest stale).
rotate() {
  local name=$1; shift
  local out=$work/rotate-$name
  rm -rf "$out"
  local env=(FLUTTER_ENGINE_SWITCHES=$#)
  local i=1
  for s in "$@"; do env+=("FLUTTER_ENGINE_SWITCH_$i=$s"); i=$((i + 1)); done
  env "${env[@]}" "$repo/target/release/aera-host-sim" --root "$work/payload" --out "$out" --until 5000 \
    --size 720x1348 --bar 165 --scale 1.75 --snap portrait@1500 --rotate@2000 --snap landscape@2800 \
    --rotate@3200 --snap back@4000 \
    >"$out.log" 2>&1
  if ! grep -q "surface now 1348x555" "$out.log" || [ ! -f "$out/landscape.png" ] || [ ! -f "$out/back.png" ]; then
    echo "rotate $name: no landscape frame" >&2; cat "$out.log" >&2; exit 1
  fi
  python3 - "$out/landscape.png" "$out/back.png" "$repo/ci" <<'PY'
import sys
sys.path.insert(0, sys.argv[3])
from png_compare import pixel, read_png
for path, size in ((sys.argv[1], (1348, 555)), (sys.argv[2], (720, 1183))):
    frame = read_png(path)
    assert (frame[0], frame[1]) == size, (path, frame[0], frame[1])
    top, bottom = pixel(frame, frame[0] // 2, 40), pixel(frame, frame[0] // 2, frame[1] - 10)
    if sum(top) >= sum(bottom):
        sys.exit(f"{path}: not upright and whole after a rotation (top {top}, bottom {bottom})")
PY
  echo "rotate $name: ok"
}
rotate gl
rotate vulkan --vulkan
