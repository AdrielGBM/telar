# Text case and underline

Two text properties that change how a text is shown without changing what it says: the case its glyphs take,
and a line drawn under them. Both flow down the tree like `font_weight`, so a container names them for every
text beneath it and a `span` for its own run.

```rsx
col text_case:upper letter_spacing:0.08em          // every label below in capitals
    text t!("nav.work")                            // written "Work" in the catalogue, shown "WORK"
    text t!("nav.contact") underline:$hovered underline_offset:4
text "Write to me" underline underline_offset:0.18em underline_thickness:2 underline_color:$theme.accent
```

| Attribute | Takes | What it does |
| --- | --- | --- |
| `text_case:` | `upper` · `lower` · `capitalize` · `none` (`uppercase`/`lowercase` too) | The case the glyphs show. `capitalize` puts the first letter of each word in title case and leaves the rest as written, as CSS does. |
| `underline` | bare, `true`/`false`, or a `$`-reading expression | Draws the line. Reading state, it follows a hover or a selection. |
| `underline_offset:` | pixels, `em` of the text's size, or a fraction of the surface | From the baseline down to the line's top edge, as CSS `text-underline-offset`. |
| `underline_thickness:` | the same lengths | How thick the line is; never thinner than a device pixel. |
| `underline_color:` | a colour | The line's colour; the text's own when unset. |

Without `underline_offset` or `underline_thickness` the line takes the face's own metrics (its `post` table),
as a browser does for `auto`. In Rust they are `TextCase` and `TextDecoration` on `TextStyle`
(`.with_text_case(…)`, `.with_underline(true)`, `.with_underline_offset(4.0)`, …) and the same builders on
`Declared`.

## The case is applied where the text is measured

A case is not a paint: `STRASSE` is wider than `straße`, so the string a target measures and the string it
draws have to be the cased one, or the box is laid out for text it does not hold. `renderer_core::case_text`
is the one mapping every measurer and shaper runs before measuring, shaping and hit-testing, so a cased label
takes the same room on every target. The text itself, and everything that reads it, keeps what was written:
the accessibility snapshot names the box by its source string, a document holds it as its text content, a
link run keeps its range, and a press on a cased run answers with the byte it was written at.

The mapping is Unicode's full case mapping, with the language-specific rules CLDR gives (`icu_casemap`):
Turkish and Azerbaijani dot their capital `İ`, Greek drops its accents in capitals, Lithuanian keeps its dots.
The language is the text's own: the nearest `lang:` on it or above it, or the active locale when none is —
the same resolution a document writes its `lang` attributes along, so the browser and the shaper choose the
same rules. A `span` may ask for its own case; it is mapped on its own, so its range still covers exactly its
words. A field (`input`) shows the case it was typed in, whatever it inherits: its caret and selection index
the value.

## On each target

| Target | Case | Underline |
| --- | --- | --- |
| web-dom | `text-transform`, under the element's `lang`. The text is measured cased by the canvas measurer, which is what `layout_parity_test.rs` holds against the browser. | The `text-decoration` longhands, with `text-decoration-skip-ink: none` because no other target lifts the line around a descender. A gradient-filled text's line takes the gradient's first stop: CSS paints a decoration in a colour only. |
| web canvas, desktop, Android, headless (GPU and software) | The shaper cases the string before shaping it. | Drawn beneath the glyphs from the shaped lines, one stroke per line per run that asks for it, its edges snapped to device pixels. |
| TUI | The cells show the cased string, measured in the same cells. | The SGR underline attribute on the run's cells. A cell has no offset, thickness or colour of its own to give the line, so those are ignored. |

## Limits

- A `span` cannot take away an underline its paragraph draws on web-dom: CSS draws a decoration across a box's
  inline children and none of them can remove it. On the other targets it can.
- A text shadow casts no shadow of the underline.
