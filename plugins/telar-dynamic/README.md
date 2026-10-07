# telar-dynamic

Runtime decoders and transports for Telar assets: the formats and the sources a Telar application resolves
at run time rather than baking at build time.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**Unlike the rest of Telar's crates, an application depends on this one directly**, alongside the
[`telar`](https://crates.io/crates/telar) facade. The facade owns the seam — what an asset transport, cache
and decoder are — and ships none of them, so nothing it carries has an opinion about which formats exist or
where bytes come from. This crate is the batteries: an implementation per format and per source, each behind
its own feature. Writing your own instead is the supported path, and costs nothing this one does not.

- API documentation: <https://docs.rs/telar-dynamic>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
