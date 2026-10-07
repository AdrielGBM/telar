# telar-icons

Iconify icons for Telar: an `icon` tag for `set:name` ids, baked at build time from local Iconify sets, your own
SVGs or a provider you name, and resolved at run time only when asked.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**An application depends on this crate directly**, alongside the [`telar`](https://crates.io/crates/telar)
facade, and lists it as a `prelude` so every `.rsx` file can write `icon`:

```toml
# Cargo.toml
telar-icons = "0.2.1"

# telar.toml
[telar]
prelude = ["telar_icons"]

[telar.icons]
iconify = "node_modules"   # `npm i -D @iconify-json/mdi`, or `@iconify/json` for every set
svg = "icons"              # icons/app/logo.svg is `app:logo`
```

```rsx
[view]
row gap:8 align:center
    icon name:"mdi:home"
    text "Home"
icon name:"app:logo" size:32 label:"Acme"
```

## The tag

`icon name:"set:name"` draws one icon in a square box of `size` px (the theme's `icon_size` when unset). An icon
drawn in one colour takes the colour of the text around it, so it follows its row's `color:`, the theme and a
hover state like the text beside it; `color:` sets one instead, and a multicolour icon keeps its own colours
unless given one. It is decoration a screen reader skips, unless `label:` names it.

## Baked by default

Icons are a format here, not a service. `cargo telar bake`, which every `cargo telar` build command runs first,
collects every literal `icon name:"…"` in the package's `.rsx`, resolves those ids and nothing else, and bakes
them into the package's artifact as it bakes a `svg src:"…"`. The application carries only the icons it draws
and needs no network or icon set to run. A `name:` given a signal or an expression cannot be baked, and is an
error naming the two ways out: choose between literal ids with `if`/`match`, or use runtime mode.

The sources, asked in this order so your own SVG can redraw a set's icon under its name:

| Key | Source |
| --- | --- |
| `svg = "icons"` | Your own SVGs, `icons/<set>/<name>.svg` |
| `iconify = "node_modules"` | Iconify JSON sets: `@iconify/json`, `@iconify-json/<set>` packages, or `<set>.json` files |
| `provider = "https://icons.example.com"` | An Iconify-compatible API the bake fetches from; nothing is fetched unless you name one |

## Licences

The bake records each baked icon's set and licence in `.telar/icons.json` and writes the notice your
application ships to `.telar/ICONS-LICENSES.txt` (a web build copies it to the site root). Public-domain and
permissive sets are accepted. An attribution, copyleft, non-commercial or undeclared licence warns, or fails the
bake with `unlisted = "fail"`, until you accept it:

```toml
[telar.icons.licenses]
allow = ["CC-BY-4.0"]   # an SPDX id accepts every set under it; a set prefix accepts that set
unlisted = "warn"       # or "fail"
```

A set of brand logos, such as `simple-icons`, gets a trademark note in the notice: its licence covers the
drawings, not the marks.

## Runtime mode

For ids the application cannot know ahead of time — a user's choice, a config file — enable the `runtime`
feature, set `[telar.icons] mode = "runtime"` (or `"both"`, which still bakes every literal), and install a
source from `telar::app!`'s setup block:

```rust
telar_icons::RuntimeIcons::provider("https://icons.example.com")
    .with_cache(telar_icons::DiskCache::new(cache_dir.join("icons")))
    .install();
```

It is built on `telar-dynamic`'s HTTP transport and caches, and any `IconSource` works in place of a provider.
Before reaching for it: an icon is only as available as the provider; every request tells the provider which
icon the user is looking at, from their address, so self-host it; without a network an icon not already in the
disk cache draws an empty box; and a browser build needs the provider in its CSP's `connect-src` and CORS headers
from the provider.

Keep it on the same version as `telar` — the same lockstep `telar` and `telar-macros` already have, and for
the same reason.

- API documentation: <https://docs.rs/telar-icons>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
