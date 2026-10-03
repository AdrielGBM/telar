# telar-platform-headless

Headless platform backend for Telar: offscreen windows for tests and benchmarks.

A `HeadlessWindow` keeps the title it was given, and `HeadlessPlatform::record_titles_into` records every one,
so a test or a prerender reads what a page is called. See
[docs/surface-title.md](https://github.com/AdrielGBM/telar/blob/main/docs/surface-title.md).

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

- API documentation: <https://docs.rs/telar-platform-headless>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
