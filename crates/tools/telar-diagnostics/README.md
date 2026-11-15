# telar-diagnostics

Line-based diagnostic types shared across the Telar `.rsx` toolchain, the semantic checks that produce them, and an optional conversion to LSP diagnostics.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** It is used
by `cargo-telar`, which prints the diagnostics in `cargo telar check`, and by `telar-analyzer`, which turns
them into editor diagnostics through the `lsp` feature. Reach for it directly only if you are writing a tool
against Telar's internals.

- API documentation: <https://docs.rs/telar-diagnostics>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
