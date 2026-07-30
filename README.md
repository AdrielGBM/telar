# Telar

[![CI](https://github.com/AdrielGBM/telar/actions/workflows/CI.yml/badge.svg)](https://github.com/AdrielGBM/telar/actions/workflows/CI.yml)
[![crates.io](https://img.shields.io/crates/v/telar.svg)](https://crates.io/crates/telar)
[![docs.rs](https://img.shields.io/docsrs/telar)](https://docs.rs/telar)
[![license](https://img.shields.io/crates/l/telar.svg)](#license)

A modular Rust UI framework with its own template language, reactive signals and a self-contained renderer.

Telar draws every pixel itself — there is no webview and no native widget toolkit underneath. Components are written in `.rsx`, an indentation-based template language that compiles to plain Rust at build time, so what ships is a single binary with no runtime interpreter.

> **Early days.** Telar is at `0.1.0`. The APIs work and are exercised by the apps in this repo, but they will keep moving before `1.0`.

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
box fill:surface stroke:border radius:16 width:300 pad:24 gap:10 direction:col
    text "{props.icon}" size:32
    text "{props.title}" size:18 color:dark
    text "{props.body}" size:14 color:muted

[preview "Fast"]
feature_card icon:"⚡" title:"Fast" body:"Software and wgpu renderers with dirty tracking."
```

A `.rsx` file has up to four sections: `[logic]` for verbatim Rust (a `pub struct Props` declares the component's props), `[style]` for constants and reusable style classes, `[view]` for the node tree, and `[preview]` blocks that the tooling can render in isolation.

## Getting started

```sh
cargo install cargo-telar
cargo add telar
```

Add a `telar.toml` next to your `Cargo.toml`:

```toml
[telar]
backend = "auto"
auto_modules = true

[telar.dev.window]
title = "my-app"
width = 1200
height = 800
```

Declare the app in `src/lib.rs` — `telar::app!` wires the theme, the startup hook, the window config and the root component:

```rust
telar::app!(
    theme::MyTheme,
    { telar::set_theme(theme::MyTheme::light()); },
    telar::AppConfig::default(),
    app::Root
);
```

Then:

```sh
cargo telar dev        # run with hot reload
cargo telar preview    # render every [preview] block, hot-reloaded
cargo telar test       # render all previews headlessly and report failures
cargo telar build --format deb   # appimage | deb | dmg | nsis | apk | dir
cargo telar doctor     # check the toolchain
```

`apps/sandbox` in this repo is the reference app and covers most of the surface. (`cargo telar new` is stubbed out and not implemented yet.)

## What's in the box

- **Reactive signals** — a fine-grained graph of signals, memos and effects; no virtual DOM, no diffing.
- **Two renderers** — a CPU rasterizer on `tiny-skia` and a GPU one on `wgpu`, both behind the same drawing vocabulary, selected by `backend = "auto" | "hardware" | "software"`.
- **Flexbox and grid layout** on top of Taffy, with reactive writing direction (LTR/RTL).
- **Motion** — tweens and springs driven by one frame ticker, with colors interpolated in Oklch.
- **Theming** — theme tokens plus light/dark mode that can follow the OS.
- **Internationalization** — translation catalogs baked at build time; `t!` validates keys and arguments at compile time.
- **Navigation** — a reactive page stack with animated transitions.
- **Hot reload** in `cargo telar dev`, and an in-app devtools overlay for inspecting the live component tree.
- **Packaging** to native installers per platform, plus Android APKs.

Targets desktop (Linux, macOS, Windows) and Android.

## Editor support

The VS Code extension provides syntax highlighting, snippets, diagnostics, completion and component preview, backed by the `telar-analyzer` language server. The extension bundles a prebuilt server binary, so no extra install step is needed.

## Crates

Everything is published under the `telar-` prefix. Most users only need the `telar` facade, which re-exports the runtime behind feature flags.

| Crate | Purpose |
| --- | --- |
| [`telar`](crates/telar) | The facade: re-exports the runtime and the `app!`/`t!` macros |
| [`cargo-telar`](crates/tools/cargo-telar) | `cargo telar` — dev server, previews, packaging |
| [`telar-reactive-core`](crates/reactive/reactive-core) | Signals, memos, effects, batching |
| [`telar-geometry-core`](crates/geometry/geometry-core) | Points, rects, transforms, border radii, Oklch color |
| [`telar-layout-core`](crates/layout/layout-core) · [`telar-layout-reactive`](crates/layout/layout-reactive) | Flexbox/grid engine and its reactive context |
| [`telar-motion-core`](crates/motion/motion-core) | Tweens, springs, the frame ticker |
| [`telar-theme-core`](crates/ui/theme-core) | Theme tokens, light/dark mode |
| [`telar-ui-core`](crates/ui/ui-core) · [`telar-ui-tree`](crates/ui/ui-tree) · [`telar-ui-components`](crates/ui/ui-components) | Widget kernel, component tree, widget catalogue |
| [`telar-renderer-core`](crates/renderer/renderer-core) | Draw commands, culling, dirty tracking |
| [`telar-renderer-software`](crates/renderer/renderer-software) · [`telar-renderer-hardware`](crates/renderer/renderer-hardware) | CPU and wgpu backends |
| [`telar-renderer-text`](crates/renderer/renderer-text) · [`telar-renderer-assets`](crates/renderer/renderer-assets) | Text shaping and glyph atlas; SVG/PNG/JPEG decoding |
| [`telar-platform-core`](crates/platform/platform-core) and `telar-platform-{winit,desktop,android,headless}` | Window/event abstraction and its backends |
| [`telar-parser`](crates/tools/telar-parser) · [`telar-transpiler`](crates/tools/telar-transpiler) · [`telar-macros`](crates/tools/telar-macros) | The `.rsx` pipeline |
| [`telar-i18n-core`](crates/i18n/i18n-core) · [`telar-navigate-core`](crates/navigate/navigate-core) · [`telar-services-core`](crates/services/services-core) | i18n runtime, navigation, platform paths and DI |
| [`telar-devtools`](crates/tools/telar-devtools) · [`telar-devtools-core`](crates/tools/devtools-core) · [`telar-diagnostics`](crates/tools/telar-diagnostics) · [`telar-workspace`](crates/tools/telar-workspace) | Devtools overlay and shared tooling |

`telar-analyzer` lives in this repo but is distributed as a binary through GitHub Releases and the VS Code extension rather than crates.io.

## Minimum supported Rust version

Rust **1.89**. Bumping it is a minor-version change.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
