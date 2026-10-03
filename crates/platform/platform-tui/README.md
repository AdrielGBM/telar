# telar-platform-tui

Terminal window, input and event loop for Telar's platform abstraction.

The window's title is written with OSC 0, and the terminal's own titles are saved on entry and given back on
exit. See [docs/surface-title.md](https://github.com/AdrielGBM/telar/blob/main/docs/surface-title.md).

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

- API documentation: <https://docs.rs/telar-platform-tui>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
