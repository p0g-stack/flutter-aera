# engine-aera: working agreements

Extends `~/.agents/AGENTS.md`. Seed; refine per directory as the nest fills in.

- `spec/bridge.md` is the contract; `aera-bridge` implements it; a drift test
  against AERA's definition guards it. Change all three together.
- Pins are recorded, not remembered: Flutter tag, engine revision, header hash,
  Mesa hash, depot_tools revision. An artifact without its pins is not releasable.
- Proof runs under the simulator. A renderer or bridge change ships with a
  simulator capture, and a device run when it touches GPU paths.
- No panics across `extern "C"`. Callbacks return errors to the platform loop.
- Renderers stay out of `surfaces`. Nothing here is linked by an app.
