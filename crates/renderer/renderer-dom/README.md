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

`src/layout_parity_test.rs` lays one tree out twice — once through Taffy, once as the CSS this crate writes —
and compares every box against `getBoundingClientRect`. It needs a real browser, so it runs on the wasm
target through `wasm-bindgen-test-runner`:

```sh
cargo test -p telar-renderer-dom --target wasm32-unknown-unknown
```

The dev shell provides the runner, a headless Chromium and the `CHROMEDRIVER` that drives it; the flags the
browser is started with are in `webdriver.json` beside this file. Set `NO_HEADLESS=1` to watch it happen.

`src/keyboard_test.rs` checks the keyboard this crate shares with the page, together with
`telar-platform-web`. It covers which keys are prevented on which focused box, which boxes are Tab stops,
and that a focus move the browser made is reported as `Event::BoxFocused`. Each focusable box carries
`data-telar-keys` (the keys it keeps), `data-telar-focus` (its identity) and a `tabindex`. See
[docs/keyboard.md](https://github.com/AdrielGBM/telar/blob/main/docs/keyboard.md).

The dev shell's chromedriver may not match its Chromium. In that case run the tests in Firefox:

```sh
nix shell nixpkgs#firefox nixpkgs#geckodriver --command bash -c \
  'unset CHROMEDRIVER; GECKODRIVER=$(which geckodriver) cargo test -p telar-renderer-dom --target wasm32-unknown-unknown'
```
