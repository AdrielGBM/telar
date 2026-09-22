# telar-semantics-core

What a thing in a Telar interface is: the roles a screen reader, a document and a terminal all describe the same box with.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

Besides roles, it defines `ConsumedKeys`, the keys a focused box keeps from its host (Tab, Space, Enter, the arrows,
paging and Home/End), with each role's default in `Role::consumed_keys`. `Semantics::focusable` carries the
set and whether the box is a Tab stop right now. See [docs/keyboard.md](https://github.com/AdrielGBM/telar/blob/main/docs/keyboard.md).

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

- API documentation: <https://docs.rs/telar-semantics-core>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
