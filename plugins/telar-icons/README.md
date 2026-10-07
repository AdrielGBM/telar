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

`icon name:"set:name"` draws one icon in a square box of `size` px (the theme's `icon_size` when unset). It is
decoration a screen reader skips, unless `label:` names it.

A monochrome icon takes the colour of the text around it, so it follows its row's `color:`, the theme and a
hover state like the text beside it; `color:` sets one instead. Which icons are monochrome follows Iconify, and is
decided when the icon is resolved, baked or at run time:

- a set whose `info` declares `"palette": false` is monochrome, every icon of it;
- a set that declares `"palette": true` keeps its colours, so a brand logo drawn in one colour stays that colour;
- a set that declares neither, such as a folder of your own SVGs, is judged per icon: monochrome when it paints in
  `currentColor`. Draw your own monochrome artwork in `currentColor`.

`color:` tints a palette icon too, as a silhouette: the renderer tints flat, drawing every paint of the artwork in
the one colour, so an icon mixing `currentColor` with fixed colours is not partly recoloured.

## Bare names

An id names its set unless `default_set` names the one a bare name is read in:

```toml
[telar.icons]
default_set = "mdi"     # `icon name:"home"` is `mdi:home`
```

The bake resolves, records and bakes a bare literal under its full id, and the transpiler reads it the same way,
even in runtime mode, so a literal reaches the application spelled in full. A bare name with no `default_set` fails
the bake and the build on its `.rsx` line. An id that only arrives as the application runs is read in the set given
to `RuntimeIcons::with_default_set` (see below), since `telar.toml` is not there to read at run time on every target.

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
application ships to `.telar/ICONS-LICENSES.txt`. Public-domain and permissive sets are accepted. An attribution,
copyleft, non-commercial or undeclared licence warns, or fails the bake with `unlisted = "fail"`, until you accept
it:

```toml
[telar.icons.licenses]
allow = ["CC-BY-4.0"]   # an SPDX id accepts every set under it; a set prefix accepts that set
unlisted = "warn"       # or "fail"
```

A set of brand logos, such as `simple-icons`, gets a trademark note in the notice: its licence covers the
drawings, not the marks.

Every `cargo telar build` ships the notice where its format keeps third-party notices, and a build whose bake
wrote none ships nothing:

| Build | Where `ICONS-LICENSES.txt` lands |
| --- | --- |
| `--target web` | The root of the site |
| `--format dir` (the default) | Beside the executable, in `target/telar-dist/<name>/` |
| `--format deb` | `/usr/share/doc/<name>/` |
| `--format appimage` | `usr/share/doc/<name>/` inside the AppDir |
| `--format dmg` | `<name>.app/Contents/Resources/` |
| `--format nsis` | The install directory, beside the `.exe`; the uninstaller removes it |
| `--format apk` | Beside the APK, as `target/telar-dist/<name>-ICONS-LICENSES.txt` |

### In the app

An APK carries only the `assets` directory its manifest names, and nothing can be added to it once cargo-apk has
signed it; a browser and an iOS app have no directory to read a file from either. So the same text is compiled into
the binary, and `telar_icons::licenses()` returns it from every platform:

```rust
if let Some(notice) = telar_icons::licenses() {
    // draw `notice` in a scrollable text on an "Open source licences" screen
}
```

It is `None` when neither the application nor a library it is built with baked an icon, so a screen can hide its
entry. The text is the one in `.telar/ICONS-LICENSES.txt`, the libraries' icons included, installed as the binary
loads, so a hot-reload dylib and a unit test find it without the application passing anything along. An icon
resolved at run time is in no notice: its licence is the provider's to state.

### Icons from libraries

A dependency's icons are baked into the dependency, and ship in your application all the same, so its notice lists
them too. A [`[telar] library`](../../docs/plugins.md#a-plugin-in-rsx) that draws icons ships the record of them
in its package, `.telar/icons-library.json`: the `include` list `cargo telar new --lib` writes names it, and
`cargo telar package` refuses a library whose package leaves it out or whose record does not match the icons it
baked. Your application's bake finds every library it is built with in cargo's dependency graph — from a
registry, a path or the workspace — and every other local crate the workspace bakes, and adds a section to the
notice for each, naming the icons that crate draws and the sets they come from.

Your `[telar.icons.licenses]` judges their sets too: a library's attribution, copyleft or undeclared set warns, or
fails the bake with `unlisted = "fail"`, until your `allow` names it. An application that draws no icon of its own
needs no source for that: a `[telar.icons]` holding only `[telar.icons.licenses]` is a policy and nothing else.

## Runtime mode

For ids the application cannot know ahead of time — a user's choice, a config file — enable the `runtime`
feature, set `[telar.icons] mode = "runtime"` (or `"both"`, which still bakes every literal), and install a
source from `telar::app!`'s setup block:

```rust
telar_icons::RuntimeIcons::provider("https://icons.example.com")
    .with_default_set("mdi")             // the set `[telar.icons] default_set` names
    .with_cache(telar_icons::DiskCache::new(cache_dir.join("icons")))
    .install();
```

It is built on `telar-dynamic`'s HTTP transport and caches, and any `IconSource` works in place of a provider.
An icon from a provider is tinted when it paints in `currentColor`, as every icon of an Iconify monochrome set
does; one from an `IconSource` is tinted by its set's `palette`, as the bake decides it.
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
