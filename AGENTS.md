# flutter-aera: working agreements

Self-contained; no external base file.

- Pattern first. Before designing a feature, check how Linux GTK and
  flutter-pi handle it, and scope ours to that.
- Standard channels only. What AERA can't do is not-implemented (or a
  documented no-op) on the standard channel. A custom channel needs a written
  reason that no standard channel covers it.
- No Dart here. Anything an app links lives elsewhere.
- `spec/host.md`, `third_party/aera/patches-api2`, the pinned upstream headers
  in `vendor/aera/` and `src/host/` change together; `tests/drift.rs` is the
  gate. The older `third_party/aera/patches` series is historical, not applied
  by this API 2 branch.
- Pins are recorded, not remembered: Flutter tag, engine revision, header
  hash, Mesa hash, AERA commit. An artifact without its pins is not releasable.
- Proof runs under the simulator. Renderer/host changes ship with captures.
  Qualification is host simulation and Cuttlefish only; never access a
  physical device, even when GPU paths change.
- No panics across `extern "C"`. Every `unsafe` block states its invariant.
