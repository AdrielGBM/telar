# Fonts as assets

A face an application ships is declared once, in `telar.toml`, and every target loads it in its own way.
Text asks for it by family like any other face: `font_family:"Display, sans_serif"` in `.rsx`, or
`TextStyle::with_font_family` in Rust.

## Declaring a face

One `[[telar.fonts]]` entry is one file, the way one `@font-face` rule is. A family with a regular and an
italic file is two entries naming the same family.

```toml
[[telar.fonts]]
family = "Display"                    # what text asks for; required
src = "assets/fonts/Display.ttf"      # relative to the package root; required
weight = [100, 900]                   # one weight, or [min, max]; default: the wght axis, else 400
style = "normal"                      # normal | italic | oblique; default normal
axes = { wght = [100, 900], opsz = [14, 32] }  # the variation axes the face offers
display = "swap"                      # web only: auto | block | swap | fallback | optional; default swap
size_adjust = 1.05                    # web only: scales the glyphs, written as size-adjust: 105%
```

- **`family` is authoritative**, as it is in CSS: it need not match the family the file declares. Native
  shapers register the face under the declared family first and keep the file's own names after it, so the
  same name works on every target.
- **Use `.ttf` or `.otf`** to reach every target. `.woff` and `.woff2` only reach a document: nothing but a
  browser unpacks them, so native builds skip them and a canvas build logs that it cannot use them.
- **Axes are declared now and animated later.** They are carried to the runtime (`FontAsset::axes`) so text
  can drive them; nothing uses them to choose a face yet.
- Every key is checked when the manifest is read: an unknown key, a weight outside 1–1000 or backwards, an
  axis tag that is not four characters, a range that runs backwards, a non-positive `size_adjust` and an
  unknown file extension are all errors naming the entry.
- A package that declares any face replaces the workspace's list whole instead of merging it.

## What each target does

| Target | What happens |
| --- | --- |
| web-dom | The packaging copies each file to `fonts/<name>-<hash>.<ext>`, lists it in `asset-manifest.json`, and writes an `@font-face` (with `font-display`, `size-adjust` when declared, `font-stretch` from a `wdth` axis) and a `<link rel="preload" as="font" crossorigin>` into `%telar.fonts%`. When the page's `document.fonts` reports `loadingdone`, the measurer forgets its cached line boxes and layout measures every text again, once. |
| web canvas | The same page. At startup the app fetches every font preload carrying `data-telar-family` (from the cache the preload filled) and adds each face to its shaper as it lands. |
| desktop, Android | `telar::app!` embeds each TTF/OTF file with `include_bytes!` and adds it to `AppConfig::fonts`, so the binary finds its faces wherever it runs. An application can also ship a file beside the executable itself: `FontAsset::file("fonts/Display.ttf")` resolves a relative path against the executable's directory. |
| TUI | Ignored: a cell has no typeface to choose. Nothing is embedded, since a terminal build shapes no glyphs. |
| headless | Like desktop, when it shapes glyphs. |

Nothing waits for a face. Text is laid out and drawn in its fallback until the face lands, then measured
again once; hydration of a prerendered page does not wait for fonts either.

## Faces that arrive later

The font database only grows. Whenever faces are added to it — a declared face fetched by a canvas page, a
face decoded by `telar_dynamic::FontDecoder`, `renderer_text::fonts::add_faces` — three things follow on the
next frame:

- every shaper, including a renderer's that was built before, takes the new faces and drops what it cached
  without them, so a `FontFamily::Stack` whose first member just arrived resolves to it;
- `renderer_core::text_metrics_generation()` moves, and `ui_core`'s `relayout_if_dirty`/`compute_layout`
  mark every measured leaf dirty, so each text is measured again once per generation;
- the loop is woken, so this happens without waiting for input.

A document build moves the same generation from the page's `loadingdone` event.

## Declaring faces in Rust

`AppConfig::fonts` takes the same concept directly:

```rust
let config = telar::AppConfig::default().with_fonts([
    telar::FontAsset::embedded(include_bytes!("../assets/fonts/Display.ttf"))
        .named("Display")
        .with_weight(telar::FontWeight::range(100, 900)),
]);
```

Loading a face does not make it the default: `AppConfig::font_family` names the family unstyled text shapes
in.
