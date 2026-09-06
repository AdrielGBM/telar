# telar-baker

Bakes `.rsx` asset references (SVG, image) into Rust source at build time, one [`Baker`] per registered
asset kind.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

This crate exists because two binaries need to bake assets without depending on each other: `cargo-telar`
bakes them while building an app, and `telar-analyzer` bakes them on its own so an IDE shows no errors for
a `src:"…"` the app itself hasn't built yet. Both link this crate instead; `telar-transpiler` only calls
into it, so a project with no baked asset never pulls in usvg/resvg/image.

- API documentation: <https://docs.rs/telar-baker>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
