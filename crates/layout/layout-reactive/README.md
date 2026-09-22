# telar-layout-reactive

Reactive layout context for Telar: signal-driven writing direction, surface size and layout state.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

`use_surface_width`/`use_surface_height`/`use_surface_size` read the current surface size as its own
signals, and `set_surface_size` writes it; the runner calls the setter before the tree is built and on every
resize. See
[docs/surface-size.md](https://github.com/AdrielGBM/telar/blob/main/docs/surface-size.md).

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

This one is an implementation detail even by that standard: it exists so two other Telar crates can share a
piece without depending on each other, and its API may change in any release.

- API documentation: <https://docs.rs/telar-layout-reactive>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
