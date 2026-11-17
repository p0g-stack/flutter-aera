# The `.aerap` layout

The package contract for the `upstream-host-api-2` branch, accepted by upstream
AERA at `1cabc509`. Existing consumer packers/pins are not changed by this spike.

## Package

A zip (stored, not deflated) of exactly two members:

| Member | What |
| --- | --- |
| `plugin.json` | the manifest |
| `runtime.xz` | the payload |

## Manifest (`plugin.json`)

Host API 2's schema (`plugin_manager.cpp`):

```json
{
  "schema": 1,
  "id": "org.example.counter",
  "name": "Counter",
  "version": "1.0.0",
  "description": "A Flutter app for AERA Recovery.",
  "icon": "plugin",
  "type": "ui-runtime",
  "entry": "main",
  "executable": "usr/bin/aera-plugin",
  "min_host_api": 2,
  "protocol_version": 2,
  "payload": "runtime.xz",
  "payload_url": "https://…/runtime.xz",
  "payload_size": 0, "payload_sha256": "…",
  "expanded_size": 0, "expanded_sha256": "…", "member_count": 0,
  "permissions": ["display", "touch-input", "desktop-surface"]
}
```

- `id`: lowercase letters, digits, `-` and `.`, 1–64, not starting or ending
  with `.`; never `browser`.
- `name` ≤ 80, `description` ≤ 320, `version` ≤ 32, `icon` ≤ 24 characters.
- `min_host_api` and `protocol_version` are both 2. `display` and
  `touch-input` are required; `desktop-surface` permits OpenDesktop.
  Upstream also allows `settings-backup`, `settings-restore`,
  `android-settings-backup`, `android-settings-restore`, `screen-mirror`.
  The old `pixel-surface`, `gpu-acceleration`, `network` and `audio-output`
  permissions are rejected by upstream's generic-plugin allowlist. All workers
  already run as root; desktop permission is not a filesystem sandbox.

### Who sets which field

| Fields | Source |
| --- | --- |
| `id`, `name`, `version`, `description`, optional `icon` | the app's `aera/plugin.json` |
| extra supported privileged permissions | the app's manifest, only when required |
| `schema`, `type`, `entry`, `executable`, `min_host_api`, `protocol_version`, `payload`, base `permissions` | API 2 values above; consumer packers still require a separate migration before adoption |
| `payload_size`, `payload_sha256`, `expanded_size`, `expanded_sha256`, `member_count` | computed by the packer from `runtime.xz` |
| `payload_url` | where `runtime.xz` is published; any https URL for local installs |

The packer also writes a copy of `plugin.json` next to the `.aerap`.

## Payload (`runtime.xz`)

xz (CRC32 check, ARM64 BCJ filter) of AERA's runtime stream, all integers
little-endian:

- `AERAWEB1`, then a u32 member count.
- Per member: a 12-byte header (`u16` name length 1–239, `u16` mode 0644
  or 0755, `u64` size), the name, then zero bytes until the **stream
  offset** (bytes since the start of the uncompressed stream) is a multiple
  of 4, then the body. There is no padding after a body.

Padding is relative to the stream, not to the member header: once a body
length is not a multiple of 4 the two differ, and AERA's reader
(`aeraui/features/browser/runtime.cpp`, `Extract()`, at abf3316) rejects
the package. Devicelab's `aera/tools/aerap.py` matches that reader.

Regular files only (AERA's mode 0, an alias whose body is the target path,
is not used), ≤4096 members. Upstream at `1cabc509` accepts ≤512 MiB per
member and ≤512 MiB compressed/expanded; it also reserves extraction RAM.

The expanded tree (`AERA_PLUGIN_ROOT`):

```
usr/bin/aera-plugin                  static launcher (launcher/)
usr/bin/aera-flutter                 the embedder, glibc
usr/bin/xdg-user-dir                 Documents and Downloads for path_provider (launcher/)
usr/lib/ld-linux-aarch64.so.1        the payload's own loader, then glibc,
usr/lib/*.so*                        libflutter_engine.so, Mesa (Zink, Turnip), libc++ …
usr/lib/libapp.so                    the AOT app (profile/release engines only)
usr/share/flutter/flutter_assets/    the app's assets (+ kernel_blob.bin for debug)
usr/share/flutter/icudtl.dat         ICU data matching the engine
usr/share/vulkan/icd.d/freedreno_icd.json  Turnip ICD (relative library_path)
usr/lib/alsa-lib/libasound_module_pcm_aera.so  ALSA's default device: AERA's audio bridge
usr/share/alsa/alsa.conf             ALSA's only configuration (audio/aera.conf)
etc/ssl/certs/ca-certificates.crt    CA bundle for Dart's HttpClient
```

The runtime kit (embedder, launcher, engine, loader, glibc, Mesa, Vulkan
loader, Turnip ICD, CA bundle) comes from flutter-aera releases; the app
contributes only `flutter_assets` and, for AOT builds, `libapp.so`.

## Storage and lifecycle

Upstream API 2 does **not** set `AERA_PLUGIN_DATA`,
`AERA_PLUGIN_DATA_VOLATILE` or the old pixel-host appearance variable.
It supplies `HOME=/tmp` and `TMPDIR=/tmp`. The embedder's optional
`AERA_PLUGIN_DATA` override remains useful for local launches, but is not a
promise made by recovery. Without that explicit override, XDG/home paths are
left to the process environment. Persistent preferences/support directories
are not qualified on this branch. An app must use an explicitly selected,
available storage path for durable data and validate its own storage policy.

The simulator may set a private output-directory override for its fixtures;
that does not establish recovery persistence. No physical device, partition,
mount, unlock or flash operation is part of this branch's proof.

Lifecycle is upstream API 2 resume=1, pause=2, stop=3. Desktop and background
mode are mutually exclusive. Leaving the plugin scene destroys the surface
and terminates the worker process group rather than retaining a Flutter app in
Recents. Desktop EXIT restores the setup orientation. There is no API 3
inactive state or live-rotation surface renegotiation; see `host.md`.

## Runtime kit

`https://github.com/p0g-stack/flutter-aera/releases/download/kit-<flutter>-<commit>/flutter-aera-kit-linux-<arch>-<mode>-<flutter>.tar.xz`
with a `.sha256` next to it, where `<commit>` is the full flutter-aera commit
the Kit workflow built; assets of a release are never replaced. Mode today:
`debug` (JIT; the
app ships `kernel_blob.bin` in `flutter_assets`). The tarball is the
expanded payload tree above without the app, plus `kit.json` (pins) at its
root, which is not packed into `runtime.xz`. To pack: extract the kit, drop
`kit.json`, add `usr/share/flutter/flutter_assets` (and `usr/lib/libapp.so`
for AOT), pack.

Kit and counter archives sort members by name, normalize ownership to numeric
root and timestamps to the producer commit time, and use single-threaded xz
with fixed compression settings. Rebuilding unchanged payload bytes therefore
does not change archives merely because the build ran later. Static simulator
executables are uploaded directly, without an archive wrapper. Push-triggered
Kit runs only validate and upload workflow artifacts; release publication
requires an explicit dispatch with `publish=true`.

`aera-plugin` binds AERA's fonts (`/twres/fonts`) at `/usr/share/fonts` and
the payload's CA bundle at `/etc/ssl/certs` in a private mount namespace,
then execs `usr/bin/aera-flutter` through `usr/lib/ld-linux-aarch64.so.1`
with `--library-path usr/lib`. The embedder finds everything else relative
to `AERA_PLUGIN_ROOT`.
