# flutter-aera: working agreements

Self-contained; no external base file.

- Pattern first. Before designing a feature, check how Linux GTK and
  flutter-pi handle it, and scope ours to that.
- Standard channels only. What AERA can't do is not-implemented (or a
  documented no-op) on the standard channel. A custom channel needs a written
  reason that no standard channel covers it.
- No Dart here. Anything an app links lives elsewhere.
- `spec/host.md`, `vendor/protocol.hpp` and `src/host/` change together; the
  drift test is the gate. Guesses about the unpublished host are marked
  `ASSUMED` in all three.
- Pins are recorded, not remembered: Flutter tag, engine revision, header
  hash, Mesa hash, AERA commit. An artifact without its pins is not releasable.
- Proof runs under the simulator; a renderer or host change ships with a
  simulator capture, and a device run when it touches GPU paths.
- No panics across `extern "C"`. Every `unsafe` block states its invariant.
