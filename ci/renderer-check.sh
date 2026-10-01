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
