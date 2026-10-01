#!/usr/bin/env bash
# Hot reload in the simulator with the stock `flutter attach`: taps the
# counter to 3, attaches to the VM service URL the embedder writes to
# plugin-data/vm-service-url, recolours the app bar and hot-reloads, then detaches.
# Passes when the colour changed and the count survived the reload.
# Run after ci/sim-smoke.sh WORKDIR (it builds the payload and the counter).
#   ci/hot-reload-check.sh WORKDIR        (needs `flutter` 3.47.5 on PATH)
set -euo pipefail
work=$(realpath -m "${1:?usage: ci/hot-reload-check.sh WORKDIR}")
repo=$(cd "$(dirname "$0")/.." && pwd)
out=$work/hot-reload
app=$work/hot-reload-app
rm -rf "$out" "$app"
cp -r "$work/counter" "$app"

"$repo/target/release/aera-host-sim" --root "$work/payload" --out "$out" --until 0 \
  --tap 980,2250@1000 --tap 980,2250@1300 --tap 980,2250@1600 --snap tapped@3000 \
  >"$out.sim.log" 2>&1 &
sim=$!
trap 'kill $sim 2>/dev/null || true' EXIT

for _ in $(seq 120); do
  [ -s "$out/plugin-data/vm-service-url" ] && [ -f "$out/tapped.png" ] && break
  sleep 0.5
done
url=$(cat "$out/plugin-data/vm-service-url")
echo "hot reload: attaching to $url"

# Drive the interactive attach through a FIFO: wait for it to be ready,
# change the app, reload, take a frame, detach, stop the sim.
fifo=$work/hot-reload.in
rm -f "$fifo" && mkfifo "$fifo"
log=$work/hot-reload.attach.log
(cd "$app" && flutter attach --debug-url "$url" -d flutter-tester <"$fifo" >"$log" 2>&1) &
attach=$!
exec 3>"$fifo"
wait_for() {
  for _ in $(seq 240); do grep -q "$1" "$log" && return 0; sleep 0.25; done
  echo "hot reload: no '$1' from flutter attach" >&2; cat "$log" >&2; exit 1
}
wait_for "Hot reload"
sed -i 's/backgroundColor: Theme.of(context).colorScheme.inversePrimary/backgroundColor: Colors.green/' "$app/lib/main.dart"
grep -q 'backgroundColor: Colors.green' "$app/lib/main.dart"
printf r >&3
wait_for "Reloaded [1-9]"
sleep 2
echo reloaded >"$out/snap.tmp" && mv "$out/snap.tmp" "$out/snap"
for _ in $(seq 40); do [ -f "$out/reloaded.png" ] && break; sleep 0.25; done
printf d >&3
exec 3>&-
wait "$attach" || true
touch "$out/stop"
wait "$sim" || true
trap - EXIT
grep "Reloaded" "$log"

python3 "$repo/ci/png_compare.py" "$out/tapped.png" "$out/reloaded.png"
echo "hot reload: ok ($out)"
