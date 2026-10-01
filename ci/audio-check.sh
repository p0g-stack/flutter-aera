#!/usr/bin/env bash
# The payload's ALSA device (audio/) against a stand-in for AERA's closed
# audio bridge: the bridge's socket, a root peer, the hello, and the PCM an
# ordinary ALSA writer (aplay) sends, converted by `plug` and paced by the
# bridge. Also: no bridge binary, or a bridge not listening, fails the open.
#   sudo ci/audio-check.sh WORKDIR      (needs libasound2-dev, alsa-utils)
set -euo pipefail
work=$(realpath -m "${1:?usage: ci/audio-check.sh WORKDIR}")/audio
repo=$(cd "$(dirname "$0")/.." && pwd)
bridge=/system/bin/aera-audio-bridge
rm -rf "$work" && mkdir -p "$work/alsa-lib"
cc -shared -fPIC -DPIC -O2 -Wall -Wextra -Werror -o "$work/alsa-lib/libasound_module_pcm_aera.so" \
  "$repo/audio/pcm_aera.c" -lasound
export ALSA_CONFIG_PATH=$repo/audio/aera.conf ALSA_PLUGIN_DIR=$work/alsa-lib

python3 - "$work" <<'EOF'
import math, struct, sys, wave
w = wave.open(sys.argv[1] + "/tone.wav", "wb")
w.setnchannels(1); w.setsampwidth(2); w.setframerate(22050)
w.writeframes(b"".join(struct.pack("<h", int(8000 * math.sin(2 * math.pi * 440 * i / 22050))) for i in range(22050)))
EOF

[ -e "$bridge" ] && { echo "$bridge exists; not touching it" >&2; exit 1; }
if aplay -q "$work/tone.wav" 2>/dev/null; then echo "opened without a bridge binary" >&2; exit 1; fi
mkdir -p /system/bin && printf '#!/bin/sh\nexit 0\n' > "$bridge" && chmod +x "$bridge"
trap 'rm -f "$bridge"; [ -n "${stand_in:-}" ] && kill "$stand_in" 2>/dev/null || true' EXIT
if aplay -q "$work/tone.wav" 2>/dev/null; then echo "opened with no bridge listening" >&2; exit 1; fi

# The stand-in: reads one stream at 48 kHz stereo S16 speed, like a sound card.
python3 - "$work/received" <<'EOF' &
import socket, struct, sys, time
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.bind(b"\0aera-browser-audio-v1"); s.listen(1)
c, _ = s.accept()
hello = struct.unpack("<4I", c.recv(16, socket.MSG_WAITALL))
n, t0 = 0, time.time()
with open(sys.argv[1], "wb") as out:
    while b := c.recv(4096):
        out.write(b); n += len(b)
        ahead = n / 192000 - (time.time() - t0)
        if ahead > 0: time.sleep(ahead)
print(f"stand-in: hello {hello}, {n} bytes in {time.time() - t0:.2f} s", flush=True)
EOF
stand_in=$!
sleep 0.5
start=$(date +%s.%N)
aplay -q "$work/tone.wav"
took=$(python3 -c "print($(date +%s.%N) - $start)")
wait "$stand_in"; stand_in=
python3 - "$work/received" "$took" <<'EOF'
import math, struct, sys
data = open(sys.argv[1], "rb").read()
frames = len(data) // 4
assert len(data) % 4 == 0, "partial frame"
# One second of tone at 48 kHz (aplay pads its last period with silence).
assert 48000 <= frames <= 48000 + 24000, frames
left = struct.unpack(f"<{frames * 2}h", data)[0::2]
right = struct.unpack(f"<{frames * 2}h", data)[1::2]
assert left == right, "mono not duplicated to both channels"
peak = max(abs(v) for v in left[:48000])
crossings = sum(1 for a, b in zip(left[:48000], left[1:48000]) if a < 0 <= b)
assert 7000 < peak < 9000, peak
assert 430 <= crossings <= 450, crossings  # 440 Hz survived the rate conversion
took = float(sys.argv[2])
assert took >= 0.9, f"aplay finished in {took:.2f} s: not paced by the bridge"
print(f"audio check: ok ({frames} frames, peak {peak}, {crossings} Hz, {took:.2f} s)")
EOF
