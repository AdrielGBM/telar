# telar-platform-winit

winit-backed window and input mapping for Telar's platform abstraction.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

This one is an implementation detail even by that standard: it exists so two other Telar crates can share a
piece without depending on each other, and its API may change in any release.

- API documentation: <https://docs.rs/telar-platform-winit>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
