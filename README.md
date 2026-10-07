# Telar

[![CI](https://github.com/AdrielGBM/telar/actions/workflows/CI.yml/badge.svg)](https://github.com/AdrielGBM/telar/actions/workflows/CI.yml)
[![crates.io](https://img.shields.io/crates/v/telar.svg)](https://crates.io/crates/telar)
[![docs.rs](https://img.shields.io/docsrs/telar)](https://docs.rs/telar)
[![license](https://img.shields.io/crates/l/telar.svg)](#license)

A modular Rust UI framework with its own template language, reactive signals and a self-contained renderer.

Telar draws every pixel itself — there is no webview and no native widget toolkit underneath. Components are written in `.rsx`, an indentation-based template language that compiles to plain Rust at build time, so what ships is a single binary with no runtime interpreter.

> **Early days.** Telar is at `0.2.1`. The APIs work and are exercised by the apps in this repo, but they will keep moving before `1.0`.

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

Already have a directory — a fresh `git init`, a Nix flake, a README you wrote first? `cargo telar init` writes the same files into it in place. It never overwrites anything: a name it would itself write that already exists is a conflict, and it refuses the whole write and lists every one rather than touching any of them (an existing `Cargo.toml` included — merge one by hand, or start from an empty directory instead).

```sh
cargo telar init            # scaffolds the current directory
cargo telar init my-app     # or a path, same as `new`
cargo telar init --target web --renderer dom   # dom | canvas | auto, browser only
```

<details>
<summary>What it writes, for adding Telar to a project you already have</summary>

```
my-app/
  Cargo.toml      # one target under [features], the widget catalogue, and the build profiles
  telar.toml      # renderer backend, the components .rsx sees, theme, catalogs, the dev window
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
prelude = ["telar_components"]

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
Four sugars survive, because each is a token shape rather than a second language:

| written | means |
| --- | --- |
| `50%` | `SizeDimension::Percent(0.5)` |
| `50sw` `50sh` `50smin` `50smax` | a fraction of the surface: `SizeDimension::SurfaceWidth(0.5)` and its siblings — see [docs/surface-size.md](docs/surface-size.md) |
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

Every `.rsx` already sees `telar`'s items and your crate root's. Components that ship in a crate of their own are made visible the same way by naming that crate once, in `telar.toml`:

```toml
[telar]
prelude = ["telar-components"]   # a package name, or a path inside one: "my_plugin::prelude"
```

Each entry is glob-imported after `telar`'s glob and before your crate's, so it has to be a dependency of the package. An explicit `use` in `[logic]` shadows every glob, which is how one file picks a name two crates both export. A workspace `telar.toml` can declare the list for every package; a package that sets its own, `[]` included, replaces it.

A directory is a module too, and `mod.rsx` is that module's file — `mod.rs` in the other language. Its `[logic]` is Rust at module level, which is where a `//!` and a `#![…]` belong; it takes no `[view]`, because a module is not callable. Give a directory one and telar declares it and everything under it, so nothing in it has to be placed by hand:

```
src/media/
  mod.rsx         # the module: docs, attributes, and Rust items
  mod.rs          # optional, and kept as it is: included into the same module
  media.rsx       # crate::media::media
  state.rs        # crate::media::state
```

Without a `mod.rsx`, a directory that holds both `.rsx` files and a hand-written `mod.rs` places them from that file with a `telar::rsx_modules!();` of its own — only a module's own file can add items to it.

`apps/sandbox` in this repo is the reference app and covers most of the surface.

## What's in the box

Everything here is either always present or one word away. Nothing is bundled.

- **Reactive signals** — a fine-grained graph of signals, memos and effects; no virtual DOM, no diffing.
- **Flexbox and grid layout** on top of Taffy, with reactive writing direction (LTR/RTL), lengths relative to the surface, and width breakpoints that re-resolve only across a threshold. → [docs/surface-size.md](docs/surface-size.md)
- **Sticky positioning** — `sticky inset_top:0` holds a box at a scroll viewport's edge inside its parent, placed by Telar on every target and by native `position: sticky` on web-dom. `sticky:$on` and `clip:$on` switch it, or a clip, by state without a rebuild. → [docs/sticky.md](docs/sticky.md)
- **Primary scroll** — a root `ScrollPage` is the page's own scroll: the document's scroll on web-dom, a drawn scroll area elsewhere, readable and movable by platform code. `use_primary_scroll()` reads it reactively from anywhere, and `use_anchor_at(line)` the anchor under a line of its view. Its arrival margin, taken from the bars of the layers fixed over it or declared, keeps an anchor followed or a control focused clear of them (`scroll-padding` on web-dom). → [docs/primary-scroll.md](docs/primary-scroll.md)
- **Fixed layer** — `layer` stands boxes against the surface over the page: out of its flow and its scroll, drawn over its sticky boxes and under every overlay, taking the pointer only over its own boxes and keeping its place in the Tab order. `position: fixed` in place on web-dom, a layout root of its own elsewhere. → [docs/fixed-layer.md](docs/fixed-layer.md)
- **Motion** — tweens and springs driven by one frame ticker, with colors interpolated in Oklch.
- **Theming** — theme tokens plus light/dark mode, reduced motion and high contrast that can follow the OS, and the user's preferred locales, read the same way on every target. → [docs/system-preferences.md](docs/system-preferences.md)
- **Keyboard** — each focusable control declares the keys it keeps, so a browser build shares Tab and scrolling with the host page instead of fighting it for them. → [docs/keyboard.md](docs/keyboard.md)
- **Internationalization** — translation catalogs baked at build time; `t!` validates keys and arguments at compile time. Full CLDR plural rules and locale-aware number and date formatting are in the plugin [`telar-i18n`](plugins/telar-i18n).
- **Two renderers** — a CPU rasterizer on `tiny-skia` and a GPU one on `wgpu`, behind the same drawing vocabulary. `desktop` and `android` bring both, and `backend = "auto"` picks per machine.
- **A widget catalogue** — buttons, fields, selects, menus, modals, tabs, sliders, and the rest — in the plugin [`telar-components`](plugins/telar-components): add it with the groups you draw (`overlays`, `chrome`, `advanced`) and list it as a `prelude` in `telar.toml`. → [docs/targets.md](docs/targets.md#the-rest-of-the-features)
- **Navigation** — a reactive page stack with animated transitions, or one stack per tab, that follows the app's address — in the plugin [`telar-navigate`](plugins/telar-navigate): add it and list it as a `prelude` in `telar.toml`. → [docs/targets.md](docs/targets.md#the-rest-of-the-features)
- **Location** — one address per app on every target: browser history with scroll restoration, `--location` deep links remembered between runs, Android `ACTION_VIEW` and the back button. → [docs/location.md](docs/location.md)
- **Surface title** — the window, tab, recents label or terminal title is derived from the app's title, the current route's title and the locale, and follows all three. → [docs/surface-title.md](docs/surface-title.md)
- **Links** — `to:` makes a box a link to a route, an anchor on the page or a URI outside the app: a real `<a href>` on web-dom, `open_uri` through the system everywhere else, OSC 8 in a terminal. `current:` marks the current one (`aria-current`). → [docs/links.md](docs/links.md)
- **Dashed strokes** — `stroke_dash:"1 4"` on a `path`, or `Stroke::with_dash` in a `canvas`, dashes an outline on every renderer: SVG `stroke-dasharray` on web-dom, `tiny-skia`'s own dashing on the CPU, dashes cut before tessellation on the GPU, dashed line characters in a terminal. → [docs/dashed-strokes.md](docs/dashed-strokes.md)
- **Images and SVG** — baked into the binary at build time out of `src:"…"`, with no parser in the binary. → `svg`
- **Icons** — `icon name:"mdi:home"` draws an [Iconify](https://iconify.design) icon in the colour of the text around it, baked at build time from Iconify sets on disk, your own SVGs or a provider you name, with each set's licence recorded in a notice — in the plugin [`telar-icons`](plugins/telar-icons).
- **Translation catalogs** baked the same way, with `t!` validating keys and arguments at compile time.

Both are baked by the CLI, and so is the `.rsx` itself: build through `cargo telar check`/`dev`/`build`/`test`, or run `cargo telar transpile` first. A plain `cargo build` fails with a message naming that command rather than compiling something stale, which is what keeps the decoders, the parser and the code generator out of every project's own build. A project that will not install the CLI produces the artifact itself from a `build.rs`, which is one call into `telar-transpiler` and gets real `cargo:rerun-if-changed` out of it.
- **Assets that arrive later**, behind a transport-agnostic reactive seam: a signal that advances `Loading` → `Ready`/`Failed`, with the transport, the cache and the decoder each yours to choose. → `async-assets`
- **Decoders and transports for that seam** — SVG, bitmaps, translation catalogs, over HTTP or from a directory — in the companion crate [`telar-dynamic`](plugins/telar-dynamic), one feature each. Yours plugs in the same way.
- **Hot reload** in `cargo telar dev`, and an in-app devtools overlay for inspecting the live component tree — or one of your own, through the same seam. *(the CLI sets this one)*
- **Packaging** to native installers per platform, plus Android APKs. → `cargo telar build --format …` For the browser, a static site from a page template with content-hashed assets, an `asset-manifest.json` and a verbatim `web/public/` → `cargo telar build --target web`, configured under `[telar.web]` ([docs/web-packaging.md](docs/web-packaging.md)). With `--prerender`, every page and locale written ahead of time, readable before the module loads and naming each element for the client that takes it over → [docs/prerender.md](docs/prerender.md). Each page carries its description, canonical, alternate-language and link-preview tags, derived from `[telar.web]`, the catalogs and the routes, with a `sitemap.xml`, a root that negotiates the reader's locale, and host profiles (`static`, `cloudflare-pages`) for the files a host reads beside the site.

The complete list, with what each feature pulls in and why, is on **[docs.rs](https://docs.rs/telar#feature-flags)**.

## Editor support

The VS Code extension provides syntax highlighting, snippets, diagnostics, completion and component preview, backed by the `telar-analyzer` language server. A component's attribute keys are completed from its props struct — names, types and doc comments — through an embedded rust-analyzer, and a diagnostic about a value lands on the `.rsx` line and column you wrote it on. The extension bundles a prebuilt server binary, so no extra install step is needed.

## Build tuning

`cargo telar new` writes profiles that keep dev builds fast and release builds small. Adding Telar to an existing workspace, wanting a faster linker, or wanting to know why **`panic = "abort"` must stay off `[profile.dev]`/`[profile.release]` but is fine in `[profile.web]`**: see **[docs/build-tuning.md](docs/build-tuning.md)**.

## Crates

You depend on one:

```toml
[dependencies]
telar = "0.2.1"
```

Everything behind it — the reactive graph, the layout engine, the renderers, the platform backends, the `.rsx` pipeline — is a separate `telar-*` crate. They are published because Cargo requires every dependency of a published crate to be published too, not because an application names them; the split is what lets a terminal build skip a GPU renderer. Reach for one directly only if you are writing a frontend or a tool against Telar's internals.

Two exceptions. [`cargo-telar`](crates/tools/cargo-telar) is a binary you install rather than a dependency. [`telar-embed`](crates/embed/telar-embed) is for hosting a separately-compiled Telar UI inside your own — or for being one. The rest is opt-in, and it is a plugin.

### Plugins

`telar` is the mechanism: traits, protocols and registries. The batteries an application may want are plugins, crates you add to your own `Cargo.toml` beside the facade, which carries none of them:

| Plugin | What it adds |
| --- | --- |
| [`telar-components`](plugins/telar-components) | The widget catalogue. No default features; groups `overlays`, `chrome` and `advanced` |
| [`telar-navigate`](plugins/telar-navigate) | The page stack: `Navigator`, the host that animates between pages, per-tab stacks |
| [`telar-watch`](plugins/telar-watch) | Filesystem watching delivered on the UI thread: `watch_path` |
| [`telar-dynamic`](plugins/telar-dynamic) | Decoders and transports for assets that arrive at run time, one feature each |
| [`telar-icons`](plugins/telar-icons) | The `icon` tag for Iconify `set:name` ids, baked from local sets, your own SVGs or a provider you name, with each set's licence recorded; feature `runtime` resolves ids as the app runs |
| [`telar-i18n`](plugins/telar-i18n) | CLDR plural rules for catalogs, and number and date formatting in the active locale, from ICU4X |
| [`telar-expression`](plugins/telar-expression) | A typed, pure expression language bound to signals |

**The layout rule.** `plugins/` holds only crates an application author adds to their own `Cargo.toml`. A crate that a `telar` feature pulls in and compiles into the target is core, even when it is opt-in, and lives in `crates/`; `crates/tools/` is host-side tooling only. Nothing under `crates/` depends on `plugins/`, which `.github/scripts/check-layering.sh` enforces in CI.

**Adding one.** Add the crate, name the feature groups you draw, and, for a plugin that ships `.rsx` tags, list it in `telar.toml` so every `.rsx` file sees them without a `use`:

```sh
cargo add telar-components --features overlays
cargo add telar-navigate
```

```toml
# telar.toml
[telar]
prelude = ["telar_components", "telar_navigate"]
```

Keep every plugin on the same version as `telar`: two copies of the kernel resolve to incompatible types. A tag missing from every `prelude`, an unknown crate and a name two entries both export are each reported on the `telar.toml` or `.rsx` line that caused them.

**Writing one.** A plugin is a Rust crate that depends on `telar` and follows the component protocol, or a `[telar] library` written in `.rsx` and published with `cargo telar package` and `cargo telar publish`. Both are covered in **[docs/plugins.md](docs/plugins.md)**.

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
| [`telar-ui-core`](crates/ui/ui-core) · [`telar-ui-tree`](crates/ui/ui-tree) | Widget kernel, component tree |
| [`telar-renderer-core`](crates/renderer/renderer-core) | Draw commands, culling, dirty tracking |
| [`telar-renderer-software`](crates/renderer/renderer-software) · [`telar-renderer-hardware`](crates/renderer/renderer-hardware) | CPU and wgpu backends |
| [`telar-renderer-tui`](crates/renderer/renderer-tui) · [`telar-renderer-dom`](crates/renderer/renderer-dom) · [`telar-renderer-web`](crates/renderer/renderer-web) | Terminal cells, browser elements, browser canvas |
| [`telar-renderer-text`](crates/renderer/renderer-text) · [`telar-renderer-assets`](crates/renderer/renderer-assets) | Text shaping and glyph atlas; SVG parsing and build-time asset baking |
| [`telar-embed`](crates/embed/telar-embed) | Embedding a separately-compiled UI, or being one — a crate an application depends on directly |
| [`telar-renderer-cache`](crates/renderer/renderer-cache) · [`telar-renderer-record`](crates/renderer/renderer-record) | The shared byte-budgeted cache; a backend that records instead of drawing |
| [`telar-platform-core`](crates/platform/platform-core) and `telar-platform-{winit,desktop,android,tui,web,headless}` | Window/event abstraction and its backends |
| [`telar-preferences-core`](crates/preferences/preferences-core) | The user's system preferences as reactive state, which theme, motion and plugins follow |
| [`telar-devtools`](crates/devtools/telar-devtools) | The dev overlay: FPS counter, node inspector, build-error banner — pulled in by `telar/dev`, and by nothing else |
| [`telar-parser`](crates/tools/telar-parser) · [`telar-transpiler`](crates/tools/telar-transpiler) · [`telar-macros`](crates/tools/telar-macros) | The `.rsx` pipeline |
| [`telar-project`](crates/tools/telar-project) | What a project *is*: `telar.toml`, source discovery, output paths, and the build artifacts a transpile leaves behind |
| [`telar-i18n-core`](crates/i18n/i18n-core) · [`telar-services-core`](crates/services/services-core) | i18n runtime, platform paths and DI |
| [`telar-reactive-local`](crates/reactive/reactive-local) | Per-surface thread-local slots, split out so `platform-core` need not link the reactive runtime |
| [`telar-icons-core`](crates/icons/icons-core) | Iconify ids, sets, sources and licence policy, shared by `telar-icons` and the baker |

The plugins are in the table above; `plugins/telar-rsx-fixture` is an unpublished test fixture, a `[telar] library` the test suite packages, unpacks read-only and compiles as a dependency.

`telar-analyzer` and `telar-diagnostics` live in this repo but are distributed through GitHub Releases and the VS Code extension rather than crates.io.

</details>

## Minimum supported Rust version

Rust **1.95**. Bumping it is a minor-version change.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
