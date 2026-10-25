# telar-wire-core

The byte spelling of every value that crosses between a Telar host and a guest library built with its own allocator.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

A hot-reloaded application and an embedded guest each link their own copy of std, so nothing one side owns
may be handed to the other to keep or free. Rich values cross as bytes instead: one side spells a value into a
buffer it owns and lends it, the other reads it back into values it allocated itself. This crate holds the
primitives (`Writer`, `Reader`), the `Wire` trait each crate implements beside its own types, and `WIRE_VERSION`,
the one number that says two builds spell alike.

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

- API documentation: <https://docs.rs/telar-wire-core>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
