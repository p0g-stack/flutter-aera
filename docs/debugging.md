# Debugging and hot reload

flutter-aera runs a debug (JIT) engine from the kit, so a debug `.aerap`
supports the stock `flutter attach`: hot reload, hot restart, DevTools. The
pieces follow GTK and flutter-pi.

| What | Where |
| --- | --- |
| Engine switches | GTK's `FLUTTER_ENGINE_SWITCHES=N` + `FLUTTER_ENGINE_SWITCH_1…N`, and, since AERA starts plugins with a fixed environment, one switch per line in `$AERA_PLUGIN_DATA/engine-switches` |
| VM service URL | logged by the engine, and written to `$AERA_PLUGIN_DATA/vm-service-url` on each start (removed at start, so it is never stale) |
| Log | `aera-plugin` sends stdout and stderr to `$AERA_PLUGIN_DATA/aera-flutter.log`, new each launch |
| Start a plugin | AERA RPC `plugin` / `open` (Host API 3 patch 0008), as tapping it in the launcher |

On AERA `$AERA_PLUGIN_DATA` is `/sdcard/AERA/plugin-data/<id>` (or
`/tmp/aera/plugin-data/<id>` while storage is not mounted).

## In the simulator

```sh
aera-host-sim --root PAYLOAD --out OUT --until 0 &   # runs until the app closes
flutter attach --debug-url "$(cat OUT/plugin-data/vm-service-url)" -d flutter-tester
```

Run `flutter attach` in the app's project. `-d flutter-tester` only gives
the tool a device to talk through: everything goes over the VM service URL.
`r` reloads, `R` restarts. While the sim runs, `echo NAME > OUT/snap` writes
`OUT/NAME.png` and `touch OUT/stop` ends it. `ci/hot-reload-check.sh` does
all of this in CI: it taps the counter to 3, recolours the app bar, reloads,
and checks the bar changed and the count did not.

## On a device over adb

With a debug `.aerap` installed (recovery's adbd runs as root):

```sh
id=org.example.counter
data=/sdcard/AERA/plugin-data/$id
# A fixed port, so the forward can be set up in advance.
adb shell "mkdir -p $data && echo --vm-service-port=8181 > $data/engine-switches"
adb forward tcp:8181 tcp:8181
# Open the plugin, as tapping it in AERA's launcher would.
adb shell 'echo "{\"v\":1,\"id\":\"dev\",\"op\":\"plugin\",\"args\":{\"action\":\"open\",\"id\":\"'$id'\"}}" > /system/bin/aerain; cat /system/bin/aeraout'
sleep 5
flutter attach --debug-url "$(adb shell cat $data/vm-service-url | tr -d '\r')" -d flutter-tester
adb shell cat $data/aera-flutter.log    # the engine's and the app's log
```

The URL carries an auth code; add `--disable-service-auth-codes` to
`engine-switches` for a fixed URL. Delete `engine-switches` to go back to
defaults.

`flutter_p0g run aera` is meant to do these steps itself.
