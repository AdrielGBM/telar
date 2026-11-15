# telar-project

The shape of a Telar project: the `telar.toml` schema, source discovery, output paths, and the names `cargo telar` and the application it launches agree on (environment variables, hot-reload lines, workshop channel files).

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** It is read
by the tools that build a project: `cargo-telar`, `telar-transpiler`, `telar-baker`, `telar-analyzer`, the
`telar-macros` proc macros, and the `telar` runtime when it reads what a dev session passed down. Reach for
it directly only if you are writing a tool against Telar's internals.

- API documentation: <https://docs.rs/telar-project>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
