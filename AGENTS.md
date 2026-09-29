# flutter-aera: working agreements

Extends `~/.agents/AGENTS.md`. Seed; refine per directory as the nest fills in.

- If stock Dart should do it on this host, it is this repo's job. If Flutter
  has no concept of it, it belongs in `surfaces`. Do not add Dart here.
- `spec/bridge.md`, `vendor/protocol.hpp` and `bridge.rs` change together;
  the drift test is the gate.
- Pins are recorded, not remembered: Flutter tag, engine revision, header
  hash, Mesa hash, depot_tools revision, AERA commit. An artifact without its
  pins is not releasable.
- Proof runs under the simulator. A renderer or bridge change ships with a
  simulator capture, and a device run when it touches GPU paths.
- No panics across `extern "C"`. Callbacks return errors to the platform loop.
