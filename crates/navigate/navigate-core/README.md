# telar-navigate-core

Page-stack navigation for Telar: a reactive navigator and an animated navigation host.

`Navigator<R>` works with any `Clone` route type. Implement [`Route`] on that type — `to_location` /
`from_location` against a platform-neutral [`Location`] (path segments, an optional in-page-anchor
fragment, query-style params; not a URL) — and `Navigator::location` / `Navigator::locations` expose the
current page and the whole stack as locations, for whichever target adapter (web history, a desktop deep
link, an Android intent, a TUI argument) serializes them further.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

- API documentation: <https://docs.rs/telar-navigate-core>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
