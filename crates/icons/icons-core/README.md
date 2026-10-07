# telar-icons-core

Iconify icon sets for Telar: icon ids, the Iconify JSON format, the icon sources the baker and the runtime both
resolve through, and the licence policy over them.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**Applications depend on [`telar-icons`](https://crates.io/crates/telar-icons), not on this crate.** It is
shared by that plugin, which draws icons and resolves them at run time, and by `telar-baker`, which resolves the
ids an application's `.rsx` names and bakes them at build time; `telar-icons` re-exports everything here an
application names. Reach for this crate directly only if you are writing a tool against Telar's internals.

- API documentation: <https://docs.rs/telar-icons-core>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
