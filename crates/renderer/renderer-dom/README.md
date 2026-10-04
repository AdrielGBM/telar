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
It also checks that focus landing on a link inside a paragraph clears the box Telar had focused, and that
what Telar draws no ring for keeps the browser's `:focus-visible` outline.

`src/audit_test.rs` renders real widgets (a button, links, a field, a checkbox, a switch, a toggle button, a
paragraph with a link run, named, translated and hidden boxes) and runs [axe-core](https://github.com/dequelabs/axe-core) over the
document; any WCAG 2.2 A/AA or best-practice violation fails it. The script comes from the dev shell, which
names it in `TELAR_AXE_CORE` (a pinned npm tarball fetched by the flake), so run it inside `nix develop`. See
[docs/accessibility.md](https://github.com/AdrielGBM/telar/blob/main/docs/accessibility.md#audits).

`src/controls_test.rs` checks a box made a control by its role, with an on/off state: a switch carries
`role="switch"` and `aria-checked`, a toggle button is a `<button>` with `aria-pressed`, and Space or Enter sent to
the focused element is kept from the page and presses the box, which the next frame writes back. See
[docs/accessibility.md](https://github.com/AdrielGBM/telar/blob/main/docs/accessibility.md#controls-and-their-state).

`src/document_scroll_test.rs` checks the surface's primary scroll as the document's own scroll: the host
grows with the content, a `window` scroll is reported as the page's `Event::BoxScrolled`, a request to move
the page scrolls the document, a scroll made before the app loaded is kept, and the surface is still measured
against the viewport. See
[docs/primary-scroll.md](https://github.com/AdrielGBM/telar/blob/main/docs/primary-scroll.md).

`src/fixed_layer_test.rs` mounts a page the document scrolls with a layer fixed over it and a sticky stage
after it: the layer's element stays inside the page where it was declared, so Tab walks its controls in that
order; it stays at the top over the stuck stage; and the browser hands it the pointer only over its own
boxes. See
[docs/fixed-layer.md](https://github.com/AdrielGBM/telar/blob/main/docs/fixed-layer.md).

`src/current_test.rs` mounts a bar fixed over a page the document scrolls, with a link to each of three sections
marked `current:` while `use_anchor_at` answers with its section: scrolling the document moves the page's offset
through `Event::BoxScrolled`, and the link to the section under the bar is the one with
`aria-current="location"`. See
[docs/links.md](https://github.com/AdrielGBM/telar/blob/main/docs/links.md#the-current-link).

The dev shell's chromedriver may not match its Chromium. In that case run the tests in Firefox:

```sh
nix shell nixpkgs#firefox nixpkgs#geckodriver --command bash -c \
  'unset CHROMEDRIVER; GECKODRIVER=$(which geckodriver) cargo test -p telar-renderer-dom --target wasm32-unknown-unknown'
```

## Blend modes (`StyledContainer::with_blend`)

`src/blend_test.rs` checks how a box's `blend` composites into the document: `mix-blend-mode` on the box
itself, and `isolation: isolate` on its parent (or the host, when the blended box is itself a layout root).
Without the isolation, `mix-blend-mode` reaches past the parent to whatever stacking context is nearest —
for an otherwise plain tree, the page itself — so a texture meant to multiply against its neighbor would
also ghost into content several levels up (see [`document.rs`](src/document.rs)'s `isolate_parent`). GPU
and software already render each layer through its own compositing pass, so they need no such fix; the
artwork path (`vector.rs`'s `Drawing::open_layer`) draws inside an `<svg>`, which isolates on its own. TUI
has no notion of a backdrop to blend against and ignores the attribute.

## Text case, underline and backdrop blur

`src/text_paint_test.rs` checks the CSS these come to and how a cased text is measured: `text-transform` with
the written text kept as the element's content, the `text-decoration` longhands (`text-underline-offset`,
`text-decoration-thickness`, `text-decoration-color`, and `text-decoration-skip-ink: none`), and
`backdrop-filter: blur()` at half the radius Telar takes. It also holds the canvas measurer's width for a cased
string, Turkish included, against what the page lays out under the same `lang`. `layout_parity_test.rs` lays
cased labels and a cased paragraph out through both engines. See
[docs/text-case-and-underline.md](https://github.com/AdrielGBM/telar/blob/main/docs/text-case-and-underline.md)
and [docs/backdrop-blur.md](https://github.com/AdrielGBM/telar/blob/main/docs/backdrop-blur.md).

## One document, two writers

What a frame says the document is — which elements, in which order, with which attributes, CSS and SVG — is
worked out once, in [`document.rs`](src/document.rs), and compiled on every target. In the browser the
reconcile brings the live document in line with it; on a host, [`prerender`](src/html.rs) writes it out as
markup, which is how `cargo telar build --target web --prerender` writes pages ahead of time. Every element a
box becomes names that box in `data-telar-id`.

`src/prerender_parity_test.rs` holds the two writers to one document: a frame reconciled into one host and
written as markup into another reads back as the same elements with the same attributes.

A host served with a prerendered page is taken over in place ([`adopt.rs`](src/adopt.rs)): the first frame
keeps every served element whose `data-telar-id`, tag and parent still match, patches the attributes, text and
shapes that differ where they stand, rebuilds only the subtree under a box that no longer fits, and sweeps what
it did not claim once the frame is done. `src/hydration_test.rs` checks it in a real browser: the same nodes
before and after, no structural mutation on a clean take-over, patches in place, a rebuilt subtree, links and
keys on the adopted elements, and the reader's scroll and focus kept. See
[docs/prerender.md](https://github.com/AdrielGBM/telar/blob/main/docs/prerender.md).
