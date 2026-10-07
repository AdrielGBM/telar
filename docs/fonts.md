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
- **Axes are what the face offers.** They are carried to the runtime (`FontAsset::axes`); text places itself
  on them with `font_variation:` (see [Variation axes and features](#variation-axes-and-features)). Nothing
  uses them to choose a face.
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

## Variation axes and features

```rsx
text "ADRIEL" font_variation:(wght $w, wdth 125, opsz 72) transition(font_variation 400ms)
col font_features:(tnum, liga 0)          // inherited by every text below
```

`font_variation:` places the face on its variation axes, each a four-letter tag and a value that may read
state; `font_features:` turns OpenType features on or off, a tag named alone meaning on. Both are text
properties that flow down the tree like `font_weight`, and like CSS a node that names either replaces what it
inherited. `wght` wins over `font_weight`. In Rust they are `FontVariations` and `FontFeatures` on
`TextStyle` and `Declared` (`FontVariations::new().with("wght", 650.0)`).

An axis value animates: `transition(font_variation 400ms)` tweens axis by axis, and a `Timeline<FontVariations>`
samples it from a scroll progress. A value that changes the text's extent (`wdth`, `wght` on most faces)
measures the text again on each change, as any restyle that moves the box does; a colour change still only
repaints.

| Target | Axes | Features |
| --- | --- | --- |
| web-dom | `font-variation-settings`, every axis | `font-feature-settings` |
| web canvas, desktop, Android, headless | `wght`, anywhere along its range. Other axes are not applied: cosmic-text instances a variable face on `wght` alone, for shaping and for the glyph cache. The text is measured and drawn at the face's default for them. | Applied by the shaper |
| TUI | Ignored: a cell has no face | Ignored |

On web-dom the text is measured by a hidden element carrying the same settings whenever it names an axis or a
feature, because a canvas `font` carries only family, size, weight and slant; a canvas measures the rest.

The case a text is shown in and the line under it are text properties too: see
[Text case and underline](text-case-and-underline.md).

## Faces that arrive later

The font database only grows. Whenever faces are added to it — a declared face fetched by a canvas page, a
face decoded by `telar_dynamic::FontDecoder`, `renderer_text::fonts::add_faces` — four things follow on the
next frame:

- every shaper, including a renderer's that was built before, takes the new faces and drops what it cached
  without them, so a `FontFamily::Stack` whose first member just arrived resolves to it;
- `renderer_core::text_metrics_generation()` moves, and `ui_core`'s `relayout_if_dirty`/`compute_layout`
  mark every measured leaf dirty, so each text is measured again once per generation;
- `use_text_metrics_generation()` moves to the same generation, before that frame lays out, so whatever reads
  it runs again (see [Text measured by hand](#text-measured-by-hand));
- the loop is woken, so this happens without waiting for input.

A document build moves the same generation from the page's `loadingdone` event.

### Text measured by hand

A text sized with `font_size:fit(…)` is measured again by layout like any other (see
[surface-size.md](surface-size.md#a-line-fitted-to-a-width)). Code that measures text itself, with
`measure_text`, reads `use_text_metrics_generation()` so it runs again when a face lands; otherwise whatever
it measured in the fallback stays:

```rust
let natural = memo(move || {
    use_text_metrics_generation();
    measure_text("ADRIEL", None, 1.0e6, &probe_style()).0
});
```

| Target | The generation moves when |
| --- | --- |
| web-dom | the page's `document.fonts` reports `loadingdone` |
| web canvas | a declared face the page fetched is added to the shaper |
| desktop, Android, headless | a face is added after startup (`add_faces`, `telar_dynamic::FontDecoder`); the faces in `AppConfig::fonts` are there before the first measure |
| TUI | never: a cell has no typeface |

## Declaring faces in Rust

`AppConfig::fonts` takes the same concept directly:

```rust
let config = telar::AppConfig::default().with_fonts([
    telar::FontAsset::embedded(include_bytes!("../assets/fonts/Display.ttf"))
        .named("Display")
        .with_weight(telar::FontWeight::range(100, 900)),
]);
```

Loading a face does not make it the default: `AppConfig::font_family` names the family text shapes in where
nothing names one; see [The default family](#the-default-family).

## The default family

`AppConfig::font_family` is the family a surface's text shapes in where nothing above it names one. It is
the surface's own, and it sits at the root of the surface's text cascade, under the theme's
`ThemeTokens::root` row: a `font_family:` declared on a box, or by the theme, wins over it the way an
author's stylesheet wins over a browser's default font.

The surface opens in the configured family, and `set_font_family` moves it while the surface runs:

```rust
telar::set_font_family(Some("JetBrains Mono".into())); // the active surface
telar::set_font_family(None);                          // back to the platform's own
```

Every text that inherits its style — every `text` in `.rsx`, every catalogue control — is measured and drawn
in the new family on the next frame. Nothing is reopened or rebuilt. `use_font_family()` reads it reactively.

| Item | What it is |
| --- | --- |
| `AppConfig::font_family` | What a surface opens in. A surface opened later carries its own configuration. |
| `set_font_family(Option<FontFamily>)` | Moves the active surface's family. Call it inside the surface: from its tree, an effect it owns, or with the surface entered (`TextureUi::enter`). A tree built again on the surface keeps it. |
| `use_font_family()` | Reactive read of the active surface's family, `None` while it is the platform's. |
| `open_surface_font_family(Option<FontFamily>)` | What the runner tells a surface before it builds the tree. A host with no runner calls it to stand in for one. |

- **The family travels in the style.** A text's resolved `TextStyle` carries it, so layout measures each
  surface's text in that surface's family and every target draws it: a document writes it as
  `font-family`, and a terminal ignores it like any other face.
- **A missing face falls back.** A named family is followed by the platform's sans-serif, the way a
  document's `font-family` list is, so a family the system lacks draws in the platform's own.
  `font_family_available` says which it will be.
- **A whole style opts out.** `Text::new` takes a complete `TextStyle`, which says the tree above has no
  business in that text, the surface's family included. `Text::declaring` starts from what the tree says.
  Every text widget has both forms, and a widget built in Rust that should follow the surface takes the
  second:

  | Complete style | Inherits, amended by a closure |
  | --- | --- |
  | `Text::new` | `Text::declaring` |
  | `Text::spanned` | `Text::spanned_declaring`, or `Text::runs` for a fixed list of runs |
  | `Input::new` | `Input::declaring` |
  | `TextArea::new` | `TextArea::declaring` |

  Each amendment is handed the inherited style and returns the final one:
  `TextArea::declaring(value, layout, |inherited| inherited.with_font_size(13.0))`.

## Listing the installed families

`font_families()` answers every family text can be set in, sorted and each once, for a picker that offers
them to `set_font_family`:

```rust
let families = memo(|| {
    use_text_metrics_generation(); // a face added later is listed too
    telar::font_families()
});
```

- **No second scan.** It reads the database the shaper already loaded, the one `font_family_available`
  asks, and keeps the list with it: asking again copies it, and every name in it is one text resolves. An
  application has no reason to load a `fontdb` of its own, which is a full scan of the system's fonts and a
  second answer that can disagree with the faces text is shaped in.
- **Each face under its first name.** That is the English one, or the family a declared face
  (`[[telar.fonts]]`, `AppConfig::fonts`) was registered under. Names starting with a dot (`.LastResort`,
  `.SF NS`) are faces a platform keeps for itself and are left out.
- **It only grows.** A face that arrives while the app runs is listed from then on; reading
  `use_text_metrics_generation()` beside it is what runs a list again when one does.
- **Shaping builds only**, like `font_family_available`: a document is drawn by the browser in its own
  fonts and a terminal has none, so neither has a database to list.
