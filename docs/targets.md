# Targets

Telar draws every pixel itself, so "which target" is a real question: a terminal build has no GPU to talk
to, a browser build has no font directory, and a desktop build has both. The answer is one word.

## One word, one target

Each target feature is complete on its own. There is no second flag to remember, no renderer to wire up,
and no row below that pays for another:

| Your app runs in | `default = [...]` | Crates compiled |
| --- | --- | --- |
| A desktop window (Linux, macOS, Windows) | `["desktop"]` | 398 |
| The terminal it was launched from | `["tui"]` | 103 |
| A browser, drawing as a document | `["web-dom"]` | 78 |
| A browser, document **and** WebGPU canvas | `["web"]` | 211 |
| Android | `["android"]` | 300 |
| Nothing — draw commands in, pixels out | `["headless"]` | 175 |

Counted with `cargo tree -p telar --no-default-features --features "<target>" -e normal --target all`,
so every platform's dependencies are in the figure at once. The spread is the point: a desktop build is
mostly wgpu and its shader toolchain, and a terminal build links neither.

`cargo telar new --target <name>` writes the manifest for you (`cargo telar init --target <name>` does the
same into a directory that already exists, refusing only the specific files it would otherwise overwrite).
Switching later is one word in `Cargo.toml`,
or `--target` on the command line for a one-off — which builds *that* target and no other: when the package
declares a feature by that name, the command turns its `default` off and names this one instead, so
`--target tui` on a project whose default is a window compiles the terminal graph alone. Everything else in
`default` comes back with it; only the other frontends are left behind.

## Desktop

```toml
[features]
default = ["desktop"]
desktop = ["telar/desktop"]
```

```sh
cargo telar dev
cargo telar build --format appimage   # deb | dmg | nsis | dir
```

A native window through winit, drawn by the GPU where there is one and the CPU where there is not.
`backend = "auto"` in `telar.toml` is what picks between them at startup; `"hardware"` or `"software"`
forces one. Both renderers are compiled in, because which machine the binary lands on is not known at build
time — an app that ships to a known fleet can drop one:

```toml
telar = { version = "0.1", default-features = false, features = ["desktop-bare", "software"] }
```

Reads the OS theme, reduced motion, high contrast and locales through winit and the platform's own APIs
(see [docs/system-preferences.md](system-preferences.md)), and its surface size from winit's `Resized`
(see [docs/surface-size.md](surface-size.md)).

## Terminal

```toml
[features]
default = ["tui"]
tui = ["telar/tui"]
```

```sh
cargo telar dev --target tui
```

The same components, laid out in whole cells and drawn with box characters and colour. No window, no GPU
and no glyph shaper — a terminal has its own font and Telar does not get a say in it, which is most of why
this build is a third of a desktop one.

Boxes, fills, strokes and text render; anything that needs subpixel geometry (gradients, shadows, arbitrary
paths, images) is approximated or dropped, because a cell is the smallest thing a terminal can colour — a
picture comes back as half blocks, two colours to a cell.

Reads `COLORFGBG` for colour scheme and the environment for locales, like Linux desktop; reduced motion and
high contrast are unknown here, a terminal keeps those to itself (see
[docs/system-preferences.md](system-preferences.md)). Its surface size is the terminal's columns and rows,
snapped to whole cells (see [docs/surface-size.md](surface-size.md)).

## Browser

```toml
[features]
default = ["web"]
web = ["telar/web"]
web-dom = ["telar/web-dom"]
```

```sh
cargo telar new my-app --target web --renderer dom   # or `init`, into an existing directory
cargo telar dev --target web              # serves on :8080
cargo telar build --target web --renderer dom
```

Two ways to draw, and they are genuinely different rather than a fallback order:

- **`web-dom`** builds the interface out of real elements laid out by CSS, measured with the browser's own
  canvas. No GPU, no shaper, no font bytes in the module — text is the browser's own, selectable and
  searchable, and screen readers see a document.
- **`web`** adds pixels on a canvas through WebGPU, which draws exactly what the desktop build draws. It
  carries a glyph shaper and needs a face, because a canvas has no fonts of its own.

With `web`, `WebRenderer::Auto` picks pixels where the browser offers a GPU adapter and a document where it
does not; the page can override at load time with `?telar-renderer=dom`. Build with `--profile web`, which
optimises for size rather than cycles — a module crosses a network before it runs an instruction.

## Android

```toml
[features]
default = ["android"]
android = ["telar/android"]
```

```sh
cargo telar dev --target android
cargo telar build --target android --format apk
```

A `NativeActivity`, with both renderers behind it. `opt-level = "z"` in `[profile.release]` is worth more
here than anywhere else.

The Android frontend is opt-in rather than implied by the target triple, because a command-line build under
Termux is an ordinary Linux process with no activity behind it, and linking one would be dead weight.

Reads colour scheme, reduced motion, high contrast and the locale list over JNI, polled every 500 ms (see
[docs/system-preferences.md](system-preferences.md)); surface size comes from the same winit `Resized` path
desktop uses (see [docs/surface-size.md](surface-size.md)).

## Headless

```toml
telar = { version = "0.1", default-features = false, features = ["headless"] }
```

No surface: `rasterize` takes draw commands and returns pixels, on the CPU. For rendering a component to
PNG from a test, a server, or a build script. `cargo telar test` is this target — it renders every
`[preview]` block and reports the ones that failed.

Nothing is read from a real user: a caller declares the `SystemPreferences` snapshot and the sizes to
resize through (see [docs/system-preferences.md](system-preferences.md) and
[docs/surface-size.md](surface-size.md)). Left undeclared, every preference is unknown and the surface keeps
the size it was created with.

## More than one target from one codebase

Naming several is allowed, and `apps/sandbox` in this repository does exactly that: one set of `.rsx` files
behind a feature per frontend, any of which `--target` builds on its own.

```toml
[features]
default = ["desktop"]
desktop = ["telar/desktop"]
tui = ["telar/tui"]
```

The cost is additive, so this is worth doing when you ship both and not worth doing to keep options open.

A build carrying two of them picks with `TELAR_TARGET` — `TELAR_TARGET=tui ./app` runs the terminal
frontend of a binary that also has a window. That is a build you asked for by naming both features
yourself: `cargo telar dev --target tui` compiles one frontend, and there is nothing left to pick between.

## The rest of the features

Everything beyond the target — the widget catalogue, navigation, SVG, i18n — is listed with what it costs
at **<https://docs.rs/telar#feature-flags>**. Most applications name a target, `components`, and nothing
else.

Decoding at runtime is not among them. `telar` draws an SVG or an image that was baked from `src:"…"`, and
holds no parser for either; reading one whose bytes arrive while the app runs — off disk, over HTTP — is
[`telar-dynamic`](https://docs.rs/telar-dynamic), a separate dependency you add on purpose, one feature per
format and transport. Nothing in the table above pays for it.
