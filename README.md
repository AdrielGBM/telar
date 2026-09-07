# Telar

[![CI](https://github.com/AdrielGBM/telar/actions/workflows/CI.yml/badge.svg)](https://github.com/AdrielGBM/telar/actions/workflows/CI.yml)
[![crates.io](https://img.shields.io/crates/v/telar.svg)](https://crates.io/crates/telar)
[![docs.rs](https://img.shields.io/docsrs/telar)](https://docs.rs/telar)
[![license](https://img.shields.io/crates/l/telar.svg)](#license)

A modular Rust UI framework with its own template language, reactive signals and a self-contained renderer.

Telar draws every pixel itself — there is no webview and no native widget toolkit underneath. Components are written in `.rsx`, an indentation-based template language that compiles to plain Rust at build time, so what ships is a single binary with no runtime interpreter.

> **Early days.** Telar is at `0.1.8`. The APIs work and are exercised by the apps in this repo, but they will keep moving before `1.0`.

## A component

```rsx
[logic]
#[derive(Default)]
pub struct Props {
    pub icon: &'static str,
    pub title: &'static str,
    pub body: &'static str,
}

[view]
box fill:$theme.surface stroke:$theme.border radius:16 width:300 pad:24 gap:10 axis:col
    text "{props.icon}" font_size:32
    text "{props.title}" font_size:18 color:$theme.dark
    text "{props.body}" font_size:14 color:$theme.muted

[preview "Fast"]
feature_card icon:"⚡" title:"Fast" body:"Software and wgpu renderers with dirty tracking."
```

A `.rsx` file has up to four sections: `[logic]` for verbatim Rust (a `pub struct Props` declares the component's props), `[style]` for reusable style classes, `[view]` for the node tree, and `[preview]` blocks that the tooling can render in isolation.

## Quick start

```sh
cargo install cargo-telar
cargo telar new my-app        # --target desktop | tui | web | android
cd my-app
cargo telar dev
```

That is the whole of the setup. `cargo telar new` writes the manifest, the build profiles, a theme, an app root and one `.rsx` component, with **one target already named** — there is nothing to wire up and no feature list to read first.

<details>
<summary>What it writes, for adding Telar to a project you already have</summary>

```
my-app/
  Cargo.toml      # one target under [features], and the build profiles
  telar.toml      # renderer backend, module discovery, the dev window
  src/main.rs     # fn main() { my_app::run(); }
  src/lib.rs      # telar::app!(…) — theme, startup hook, config, root
  src/theme.rs    # the design tokens every component reads
  src/app.rs      # the root component
  src/home.rsx    # one page
```

`src/lib.rs` is the whole of the wiring:

```rust
telar::app!(
    theme::AppTheme,
    { telar::set_theme(theme::AppTheme::light()); },
    telar::AppConfig::default(),
    app::Root
);
```

and `telar.toml` sits next to `Cargo.toml`:

```toml
[telar]
backend = "auto"
auto_modules = true

[telar.dev.window]
title = "my-app"
width = 1000
height = 700
```

</details>

Then:

```sh
cargo telar dev        # run with hot reload
cargo telar preview    # render every [preview] block, hot-reloaded
cargo telar preview --png out/   # render each one to a PNG instead, with no window
cargo telar test       # render all previews headlessly and report failures
cargo telar check      # type-check, with .rsx errors on the lines you wrote
cargo telar build --format deb   # appimage | deb | dmg | nsis | apk | dir
cargo telar doctor     # check the toolchain
```

### One target, one word

Telar is a set of small crates behind one facade, and a build carries the target it named and nothing else:

| Your app runs in | `default = [...]` | Crates compiled |
| --- | --- | --- |
| A desktop window (Linux, macOS, Windows) | `["desktop"]` | 397 |
| The terminal it was launched from | `["tui"]` | 102 |
| A browser, drawing as a document | `["web-dom"]` | 77 |
| A browser, document **and** WebGPU canvas | `["web"]` | 210 |
| Android | `["android"]` | 299 |
| Nothing — draw commands in, pixels out | `["headless"]` | 174 |

Counted with `cargo tree -p telar --no-default-features --features "<target>" -e normal --target all`, so every platform's dependencies are in the figure at once. Every row is complete on its own: naming it is the whole of the choice, and no row pays for another — a desktop build is mostly wgpu and its shader toolchain, and a terminal build links neither. Switching later is one word in `Cargo.toml`.

Per-target guides, and how to ship two targets from one codebase, are in **[docs/targets.md](docs/targets.md)**.

### One value grammar

An attribute is `key` — a flag that asserts itself — or `key:<rust expression>`, read to the next space at
delimiter depth 0. **Every value is Rust**, evaluated in the generated scope, so a call, a path, a method
chain, a macro or a closure is itself and rustc judges it against the line you wrote:

```rsx
col gap:8 pad:(space::lg() * 2.0) align:center
    btn "Save" on_press:(|| draft.save()) fill:$theme.primary
    text "Hola {name}" font_size:14
```

Parenthesise an expression that holds a space — `(a + b)` *is* an expression, so nothing new is invented.
Three sugars survive, because each is a token shape rather than a second language:

| written | means |
| --- | --- |
| `50%` | `SizeDimension::Percent(0.5)` |
| `#3d78fa` | `Color::rgba(…)` |
| `$sig` | `sig.get()` — a read of anything reactive, including the `theme` handle the view binds |

`key(…)` is reserved for the handful of **directives** that have a grammar of their own and are not Rust at
all: `transition(fill 250ms ease-out)` is a clause list, `hover_style(fill:$theme.accent)` a nested
attribute list. The spelling says which world you are in.

**Reactivity is reading, not marking.** A layout value that is not a literal is re-resolved whenever what it
reads changes — `pad:$theme.gutter` follows a theme switch, and so does `pad:gutter()`. The `$` is `.get()`
sugar; it does not decide anything.

### Components are Rust paths

A `.rsx` file is a module, and its component is the `pub fn` named after the file. Import what you call:

```rsx
[logic]
use crate::ui::card::{card, CardProps};

[view]
card pad:20
    text "inside" font_size:14
```

`apps/sandbox` in this repo is the reference app and covers most of the surface.

## What's in the box

Everything here is either always present or one word away. Nothing is bundled.

- **Reactive signals** — a fine-grained graph of signals, memos and effects; no virtual DOM, no diffing.
- **Flexbox and grid layout** on top of Taffy, with reactive writing direction (LTR/RTL).
- **Motion** — tweens and springs driven by one frame ticker, with colors interpolated in Oklch.
- **Theming** — theme tokens plus light/dark mode that can follow the OS.
- **Internationalization** — translation catalogs baked at build time; `t!` validates keys and arguments at compile time.
- **Two renderers** — a CPU rasterizer on `tiny-skia` and a GPU one on `wgpu`, behind the same drawing vocabulary. `desktop` and `android` bring both, and `backend = "auto"` picks per machine.
- **A widget catalogue** — buttons, fields, selects, menus, modals, tabs, sliders, and the rest. → `components`
- **Navigation** — a reactive page stack with animated transitions. → `navigate`
- **Images and SVG** — baked into the binary at build time out of `src:"…"`, with no parser in the binary. → `svg`
- **Translation catalogs** baked the same way, with `t!` validating keys and arguments at compile time.

Both are baked by the CLI, and so is the `.rsx` itself: build through `cargo telar check`/`dev`/`build`/`test`, or run `cargo telar transpile` first. A plain `cargo build` fails with a message naming that command rather than compiling something stale, which is what keeps the decoders, the parser and the code generator out of every project's own build. A project that will not install the CLI produces the artifact itself from a `build.rs`, which is one call into `telar-transpiler` and gets real `cargo:rerun-if-changed` out of it.
- **Assets that arrive later**, behind a transport-agnostic reactive seam: a signal that advances `Loading` → `Ready`/`Failed`, with the transport, the cache and the decoder each yours to choose. → `async-assets`
- **Decoders and transports for that seam** — SVG, bitmaps, translation catalogs, over HTTP or from a directory — in the companion crate [`telar-dynamic`](crates/telar-dynamic), one feature each. Yours plugs in the same way.
- **Hot reload** in `cargo telar dev`, and an in-app devtools overlay for inspecting the live component tree. *(the CLI sets this one)*
- **Packaging** to native installers per platform, plus Android APKs. → `cargo telar build --format …`

The complete list, with what each feature pulls in and why, is on **[docs.rs](https://docs.rs/telar#feature-flags)**.

## Editor support

The VS Code extension provides syntax highlighting, snippets, diagnostics, completion and component preview, backed by the `telar-analyzer` language server. A component's attribute keys are completed from its props struct — names, types and doc comments — through an embedded rust-analyzer, and a diagnostic about a value lands on the `.rsx` line and column you wrote it on. The extension bundles a prebuilt server binary, so no extra install step is needed.

## Build tuning

`cargo telar new` writes profiles that keep dev builds fast and release builds small. Adding Telar to an existing workspace, wanting a faster linker, or wanting to know why **`panic = "abort"` must stay off**: see **[docs/build-tuning.md](docs/build-tuning.md)**.

## Crates

You depend on one:

```toml
[dependencies]
telar = "0.1.8"
```

Everything behind it — the reactive graph, the layout engine, the renderers, the platform backends, the `.rsx` pipeline — is a separate `telar-*` crate. They are published because Cargo requires every dependency of a published crate to be published too, not because an application names them; the split is what lets a terminal build skip a GPU renderer. Reach for one directly only if you are writing a frontend or a tool against Telar's internals.

Two exceptions. [`cargo-telar`](crates/tools/cargo-telar) is a binary you install rather than a dependency. And [`telar-dynamic`](crates/telar-dynamic) is a second dependency, for an application that decodes an asset at run time rather than baking it: the facade owns the seam and ships no implementation of it, so the decoders and transports live there, one feature each.

<details>
<summary><b>The crates behind the facade</b></summary>

| Crate | Purpose |
| --- | --- |
| [`telar-reactive-core`](crates/reactive/reactive-core) | Signals, memos, effects, batching |
| [`telar-geometry-core`](crates/geometry/geometry-core) | Points, rects, transforms, border radii, Oklch color |
| [`telar-layout-core`](crates/layout/layout-core) · [`telar-layout-reactive`](crates/layout/layout-reactive) | Flexbox/grid engine and its reactive context |
| [`telar-motion-core`](crates/motion/motion-core) | Tweens, springs, the frame ticker |
| [`telar-theme-core`](crates/ui/theme-core) | Theme tokens, light/dark mode |
| [`telar-semantics-core`](crates/semantics/semantics-core) | What a thing in an interface *is*, for screen readers, documents and terminals |
| [`telar-ui-core`](crates/ui/ui-core) · [`telar-ui-tree`](crates/ui/ui-tree) · [`telar-ui-components`](crates/ui/ui-components) | Widget kernel, component tree, widget catalogue |
| [`telar-renderer-core`](crates/renderer/renderer-core) | Draw commands, culling, dirty tracking |
| [`telar-renderer-software`](crates/renderer/renderer-software) · [`telar-renderer-hardware`](crates/renderer/renderer-hardware) | CPU and wgpu backends |
| [`telar-renderer-tui`](crates/renderer/renderer-tui) · [`telar-renderer-dom`](crates/renderer/renderer-dom) · [`telar-renderer-web`](crates/renderer/renderer-web) | Terminal cells, browser elements, browser canvas |
| [`telar-renderer-text`](crates/renderer/renderer-text) · [`telar-renderer-assets`](crates/renderer/renderer-assets) | Text shaping and glyph atlas; SVG parsing and build-time asset baking |
| [`telar-dynamic`](crates/telar-dynamic) | Runtime asset decoders and transports — the one crate here an application depends on directly |
| [`telar-renderer-cache`](crates/renderer/renderer-cache) · [`telar-renderer-record`](crates/renderer/renderer-record) | The shared byte-budgeted cache; a backend that records instead of drawing |
| [`telar-platform-core`](crates/platform/platform-core) and `telar-platform-{winit,desktop,android,tui,web,headless}` | Window/event abstraction and its backends |
| [`telar-parser`](crates/tools/telar-parser) · [`telar-transpiler`](crates/tools/telar-transpiler) · [`telar-macros`](crates/tools/telar-macros) | The `.rsx` pipeline |
| [`telar-i18n-core`](crates/i18n/i18n-core) · [`telar-navigate-core`](crates/navigate/navigate-core) · [`telar-services-core`](crates/services/services-core) | i18n runtime, navigation, platform paths and DI |
| [`telar-reactive-local`](crates/reactive/reactive-local) | Per-surface thread-local slots, split out so `platform-core` need not link the reactive runtime |

`telar-analyzer` and `telar-diagnostics` live in this repo but are distributed through GitHub Releases and the VS Code extension rather than crates.io.

</details>

## Minimum supported Rust version

Rust **1.95**. Bumping it is a minor-version change.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
