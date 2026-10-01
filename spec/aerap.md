# The `.aerap` layout

What `flutter_p0g` produces and AERA installs. Stable for building against
the assumed host; fields marked **ASSUMED** follow Host API 3 when AERA
publishes it.

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
- `min_host_api` 3, `protocol_version` 3, permissions `pixel-surface` and
  `gpu-acceleration`: **ASSUMED** (Host API 2 accepts only 2 and its own
  permission list). `display` and `touch-input` are Host API 2's required
  pair. Add `network` or `audio-output` only when the app needs them
  (**ASSUMED** names).

## Payload (`runtime.xz`)

xz (CRC32 check, ARM64 BCJ filter) of AERA's runtime stream: `AERAWEB1`, a
little-endian u32 member count, then per member a `u16 name length`,
`u16 mode` (0644 or 0755), `u64 size`, the name, padding to 4 bytes, the
bytes. Regular files only, no symlinks, ≤ 4096 members, ≤ 100 MiB each,
≤ 512 MiB compressed and expanded.

The expanded tree (`AERA_PLUGIN_ROOT`):

```
usr/bin/aera-plugin                  static launcher (launcher/)
usr/bin/aera-flutter                 the embedder, glibc
usr/lib/ld-linux-aarch64.so.1        the payload's own loader, then glibc,
usr/lib/*.so*                        libflutter_engine.so, Mesa (Zink, Turnip), libc++ …
usr/lib/libapp.so                    the AOT app (profile/release engines only)
usr/share/flutter/flutter_assets/    the app's assets (+ kernel_blob.bin for debug)
usr/share/flutter/icudtl.dat         ICU data matching the engine
etc/ssl/certs/ca-certificates.crt    CA bundle for Dart's HttpClient
```

`aera-plugin` binds AERA's fonts (`/twres/fonts`) at `/usr/share/fonts` and
the payload's CA bundle at `/etc/ssl/certs` in a private mount namespace,
then execs `usr/bin/aera-flutter` through `usr/lib/ld-linux-aarch64.so.1`
with `--library-path usr/lib`. The embedder finds everything else relative
to `AERA_PLUGIN_ROOT`.
