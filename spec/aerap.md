# The `.aerap` layout

What `flutter_p0g` produces and AERA installs with the Host API 3 patches
(`third_party/aera/`, patch 0005).

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
  "min_host_api": 3,
  "protocol_version": 3,
  "payload": "runtime.xz",
  "payload_url": "https://…/runtime.xz",
  "payload_size": 0, "payload_sha256": "…",
  "expanded_size": 0, "expanded_sha256": "…", "member_count": 0,
  "permissions": ["display", "touch-input", "pixel-surface", "gpu-acceleration"]
}
```

- `id`: lowercase letters, digits, `-` and `.`, 1–64, not starting or ending
  with `.`; never `browser`.
- `name` ≤ 80, `description` ≤ 320, `version` ≤ 32, `icon` ≤ 24 characters.
- `min_host_api` 3 and `protocol_version` 3 make it a pixel plugin.
  Permissions: `display`, `touch-input` and `pixel-surface` are required;
  `gpu-acceleration`, `network` and `audio-output` are the only others
  allowed. They are declarations shown to the user: plugins run as root and
  nothing enforces them. Add `network` or `audio-output` only when the app
  needs them.

### Who sets which field

| Fields | Source |
| --- | --- |
| `id`, `name`, `version`, `description`, optional `icon` | the app's `aera/plugin.json` |
| extra `permissions` (`network`, `audio-output`) | the app's `aera/plugin.json`, merged after the fixed four |
| `schema`, `type`, `entry`, `executable`, `min_host_api`, `protocol_version`, `payload`, base `permissions` | fixed by the packer, as above |
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
is not used), ≤ 4096 members, ≤ 100 MiB each, ≤ 512 MiB compressed and
expanded.

The expanded tree (`AERA_PLUGIN_ROOT`):

```
usr/bin/aera-plugin                  static launcher (launcher/)
usr/bin/aera-flutter                 the embedder, glibc
usr/lib/ld-linux-aarch64.so.1        the payload's own loader, then glibc,
usr/lib/*.so*                        libflutter_engine.so, Mesa (Zink, Turnip), libc++ …
usr/lib/libapp.so                    the AOT app (profile/release engines only)
usr/share/flutter/flutter_assets/    the app's assets (+ kernel_blob.bin for debug)
usr/share/flutter/icudtl.dat         ICU data matching the engine
usr/share/vulkan/icd.d/freedreno_icd.json  Turnip ICD (relative library_path)
etc/ssl/certs/ca-certificates.crt    CA bundle for Dart's HttpClient
```

The runtime kit (embedder, launcher, engine, loader, glibc, Mesa, Vulkan
loader, Turnip ICD, CA bundle) comes from flutter-aera releases; the app
contributes only `flutter_assets` and, for AOT builds, `libapp.so`.

## Runtime kit

`https://github.com/p0g-stack/flutter-aera/releases/download/kit-<flutter>/flutter-aera-kit-linux-arm64-<mode>-<flutter>.tar.xz`
with a `.sha256` next to it. Today: `kit-3.47.5`, mode `debug` (JIT; the
app ships `kernel_blob.bin` in `flutter_assets`). The tarball is the
expanded payload tree above without the app, plus `kit.json` (pins) at its
root, which is not packed into `runtime.xz`. To pack: extract the kit, drop
`kit.json`, add `usr/share/flutter/flutter_assets` (and `usr/lib/libapp.so`
for AOT), pack.

`aera-plugin` binds AERA's fonts (`/twres/fonts`) at `/usr/share/fonts` and
the payload's CA bundle at `/etc/ssl/certs` in a private mount namespace,
then execs `usr/bin/aera-flutter` through `usr/lib/ld-linux-aarch64.so.1`
with `--library-path usr/lib`. The embedder finds everything else relative
to `AERA_PLUGIN_ROOT`.
