# Accessibility: names, languages and hiding

Most of what a screen reader needs Telar derives on its own: the role of a control comes from what it
does or from `role:`, its state from the widget, and its name from the text it draws. Three things only
the application knows, and it says them with three attributes that any built-in tag takes (`text`, `col`,
`row`, `grid`, `box`, `img`, `svg`, `path`, `canvas`, `scroll`, `input`):

| Attribute | Value | Means |
| --- | --- | --- |
| `label:` | `"text"`, `t!("key")`, or an expression reading `$state` | The name assistive technology reads for the box. Re-read when what it reads changes, so a translated name follows the locale. |
| `lang:` | a BCP 47 tag: `"ja"`, `"es-CL"` | The language of the box and everything under it. The nearest one wins. |
| `a11y:hidden` | — | Assistive technology skips the box and its whole subtree. It is still drawn and still answers the pointer. |

On a component tag (`button`, `chip`, …) `label` stays the component's own prop, and `lang`/`a11y` are
not taken: a component forwards them to its own root if it wants to.

From Rust the same three are methods of `telar::Accessible`, which every widget with a layout node has:
`.a11y_label(|| …)`, `.a11y_lang(|| …)`, `.a11y_hidden()`.

## Pictures

An `img` or `svg` without `label` is decoration and is hidden from assistive technology. With one, it is
an image with that name.

```text
svg src:icon                       // decoration
svg src:icon label:"Telar logo"    // "Telar logo, image"
```

## Split letters

A word drawn one box per letter — for a stagger, a per-letter colour — would otherwise be read as five
letters. The parent names the word and each letter is hidden:

```text
row label:"Telar" role:h2
    for letter in letters.clone()
        text "{letter}" a11y:hidden
```

A name does not replace a box's content: a reader that walks into the box still finds what is drawn
there, which is why the letters are hidden rather than left for the name to cover.

## What each target does with them

| Target | `label` | `lang` | `a11y:hidden` |
| --- | --- | --- | --- |
| Browser, document (`web-dom`) | `aria-label`. A plain `div` may not carry a name, so a named box with no role of its own gets `role="group"`; a picture is an `<svg role="img">`, where `aria-label` is its `alt`. | `lang` | `aria-hidden="true"` |
| Desktop | AccessKit node label. A named control is called that instead of by the text it draws; a named box that is not a control is a label, or an image when it draws only artwork. | AccessKit `language` | The subtree, controls included, is left out of the tree. |
| Terminal | Used in the plain-text reading (below). | Not in the reading: plain text has no voice to switch. | Left out of the reading. |
| Android | Not yet: Telar has no Android accessibility bridge. The snapshot already carries all three, and a bridge maps them to `contentDescription`, the locale of the node and `importantForAccessibility="no"`. | — | — |
| Browser, canvas (`web`) and headless | The canvas has no accessibility tree to hand them to; headless has no reader. The snapshot (`ui_core::accessibility::snapshot`) still resolves them, which is what the tests read. | — | — |

`lang:` is per subtree. The surface's own language — the root the subtrees differ from — is the active
locale's (`use_locale()`), and each target says it in its own idiom:

| Target | The surface's own language |
| --- | --- |
| Browser, document and canvas (`web-dom`, `web`) | `<html lang>`, kept in sync with the active locale at runtime; `<html dir>` follows it the same way `layout_core::Direction::for_locale` resolves the writing direction, and both start from `[telar.web]`'s packaged page (see [web-packaging.md](web-packaging.md)). |
| Desktop | The AccessKit root node's language. A node under it that carries no `lang:` of its own inherits this — the nearest one wins, root included. |
| Terminal | Nothing to set: the reading is plain text, and a reading has no voice to switch (as above). |
| Android | Not yet: same as `lang:` itself, there is no accessibility bridge to carry it. The intended target is the view's own locale (`Configuration.locale` via JNI) once one exists. |
| Headless | No reader, so nothing to carry it to. |

## The terminal's plain-text reading

A terminal screen reader reads the cells, and cells carry neither a name the application gave nor what it
hid. Set `TELAR_TUI_READING` to a file path (or `TuiPlatformConfig::reading`) and the terminal frontend
keeps that file as a reading of the screen, rewritten whenever it changes: one line per thing on screen,
in reading order, named the way a screen reader would name it (`Close, button`, `Telar logo, image`).
Following it from a second terminal gives a reader what the cells cannot. Off by default, so a frame costs
nothing when nobody reads it.

```sh
TELAR_TUI_READING=/tmp/reading.txt cargo telar dev --target tui
tail -F /tmp/reading.txt
```

The reading ends the line of whatever holds the keyboard with `, focused` (`Save, button, focused`), since a
reader following the file has no ring to look at.

## Names the catalogue gives

A `text_field` is named by its `label`, or by its `placeholder` when it has no label: the caption drawn above
the box is not inside the line a reader lands on, so without the name the document's entry announced only
"Text field" and the desktop read out whatever had been typed. `label:` (`a11y_label`) with an empty string is
no name at all: the box goes by what it draws, as if nothing had been said.

## Contrast of the defaults

The tokens a theme leaves to `ThemeTokens` meet WCAG AA (4.5:1) for the text the catalogue draws with them.
`primary` and `on_primary` follow the light/dark mode, since one blue cannot carry white text and also read as
text on a dark surface; `muted` is opaque and mode-following, so a caption or a hint reads on `surface` and on
`surface_alt` over it; and a field's placeholder keeps 70% of the field's ink rather than half. A theme that
overrides a token owns its contrast, and the audit below is the way to check it.

## Audits

Two tests hold every target to the same fixture: a heading, a button, a link to an external address and one
to a route, a labelled field, a checkbox, a paragraph with a link run, a word named with `label:` over hidden
letters, a quotation in another `lang:` and a box under `a11y:hidden`.

| Test | What it checks |
| --- | --- |
| `crates/renderer/renderer-dom/src/audit_test.rs` | The document those widgets render to, audited by [axe-core](https://github.com/dequelabs/axe-core) with its WCAG 2.2 A/AA and best-practice rules (`region` off: the fixture is a fragment of a page). Run with nothing focused, with the field focused and with the button focused, and any violation fails the test with the rule, the element and the reason. It also checks that the keyboard's box wears Telar's ring in the document. |
| `crates/platform/platform-desktop/src/accessibility_test.rs` (`from_a_screen`) | The AccessKit tree a desktop window publishes for the same screen: each control's role and name, that it takes `Focus` and `Click`, the URL of each link and link run, the checkbox's state, the language and the hidden boxes, and that the tree's focus follows the Tab order. |

axe-core is MPL-2.0 and is not vendored into the repository. The flake fetches the published npm tarball by its
integrity hash and puts `axe.min.js` and its `LICENSE` in the store, and the dev shell names the script in
`TELAR_AXE_CORE`, which the test reads at compile time. Moving to another release is a change to the URL and the
hash in `flake.nix`. Run it in Firefox:

```sh
nix develop -c nix shell nixpkgs#firefox nixpkgs#geckodriver -c sh -c \
  'export GECKODRIVER=$(which geckodriver); unset CHROMEDRIVER; cargo test -p telar-renderer-dom --target wasm32-unknown-unknown --test audit'
```

Where the keyboard is shown on each target is in [keyboard.md](keyboard.md#what-shows-where-the-keyboard-is).

The desktop tree is still flat and carries no structure: the snapshot is built from the controls and the text a
frame draws, and a raster frame does not say which box is a heading or a landmark, so the fixture's heading
reaches a desktop reader as a label. The document has the `<h1>`.
