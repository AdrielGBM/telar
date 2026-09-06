# telar-renderer-dom

Document renderer for Telar: a frame reconciled into real DOM elements laid out by CSS.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

- API documentation: <https://docs.rs/telar-renderer-dom>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>

## Running the browser test

`tests/layout_parity.rs` lays one tree out twice — once through Taffy, once as the CSS this crate writes —
and compares every box against `getBoundingClientRect`. It needs a real browser, so it runs on the wasm
target through `wasm-bindgen-test-runner`:

```sh
cargo test -p telar-renderer-dom --target wasm32-unknown-unknown
```

The dev shell provides the runner, a headless Chromium and the `CHROMEDRIVER` that drives it; the flags the
browser is started with are in `webdriver.json` beside this file. Set `NO_HEADLESS=1` to watch it happen.
