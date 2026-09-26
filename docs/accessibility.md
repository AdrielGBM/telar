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
