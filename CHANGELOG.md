# Changelog

## 0.2.0 — unreleased

The `.rsx` pipeline had grown a second, weaker type system beside Rust's: a table describing every callee so
a call site could shape values, three namespaces a bare name might resolve through, and a sigil that decided
what counted as reactive. This release deletes all three. **A value is a Rust expression**, and rustc judges
it against the `.rsx` line you wrote.

### Migrating

`cargo telar migrate` does the mechanical half and is idempotent, so `--check` tells you whether a project is
already converted:

```sh
cargo telar migrate           # rewrite this project's .rsx files
cargo telar migrate --check   # or just report what would change
cargo telar fmt
cargo test --workspace
```

It reports, rather than guesses, every `build "…"` and `widget "…"` site. Turning
`build "tray_icon(item, config, fg, size)?"` into a tag needs *names* for four positional arguments, and only
you know them — see **The child position is closed** below.

### The value grammar

An attribute is now `key` (a flag) or `key:<rust expression>`, read to the next space at delimiter depth 0.
Parenthesise an expression that holds a space; `(a + b)` is an expression, so nothing new is invented.

| was | is | why |
| --- | --- | --- |
| `gap(8)`, `label("Save")` | `gap:8`, `label:"Save"` | `key(…)` meant three different things at once |
| `on_press(\|\| f())` | `on_press:(\|\| f())` | one introducer for every value |
| `label:t"buttons.save"` | `label:t!("buttons.save")` | the macro the Rust side already uses, with its compile-time key check |
| `fill:theme.primary` | `fill:$theme.primary` | a theme read is a read, and `$` is what a read is spelled with |
| `clip:x` | `clip:Clip::x()` | a clip is a shape — axis, radius and inset — not one of three axis keywords |
| `[style]` constants | `[logic]` `const`s | `[style]` keeps classes; a constant is Rust and `[logic]` is Rust |

`key(…)` survives for the handful of **directives** that are not Rust at all and have a parser of their own:
`transition(fill 250ms ease-out)`, `hover_style(…)`, `active_style(…)`, `disabled_style(…)`,
`focus_style(…)`, `cols(…)`, `stroke_width(…)`, `drag_button(…)`.

Three sugars survive, each a token shape rather than a second language: `50%`, `#3d78fa`, and `$sig` for a
read. Interpolation stays too — `text "Hola {name}"` expands to a `format!`, with each hole a Rust
expression.

### Reactivity is reading, not marking

A layout value that is not a literal is re-resolved whenever what it reads changes. `pad:$theme.gutter`
follows a theme switch — it did not before, silently — and so does `pad:gutter()`. The `$` is `.get()` sugar;
it no longer decides anything about a layout attribute.

The cost is one effect on a node that has a computed layout value and reads nothing: created once, run once,
never woken. Nodes whose values are all literals are untouched.

### Components are Rust paths

Each `.rsx` is a module where its file sits, and the component is the `pub fn` named after the file.
`rsx_modules!` no longer re-exports every component at the crate root, so a caller imports what it uses:

```rsx
[logic]
use crate::ui::card::{card, CardProps};
```

Props are set through the typed builder `#[derive(Props)]` generates, so a forgotten required prop is a
compile error naming that prop, and a misspelled one is "no method named …" on your own line. Props structs
are `Clone`; a handler prop is an `Rc<dyn Fn…>` rather than a `Box`, which is what lets a props value reach a
region that rebuilds.

### The child position is closed

`build "…"` and `widget "…"` are gone. A child is an element, full stop. `canvas paint:draw` is the tag the
`widget` escape mostly existed for; everything else becomes a component with named props.

### Breaking API changes

- `ClippedItem::along(item, ClipAxis::…)` → `ClippedItem::new(item, Clip::…)`. `Clip` carries the axis, a
  radius and an inset; `Clip::both()`, `Clip::x()`, `Clip::y()`, then `.rounded(r)` / `.inset(n)`.
- Handler props on the widget catalogue are `Rc<dyn Fn…>`, not `Box<dyn Fn…>`.
- `context_menu::Entry::Custom` holds a recipe (`Rc<dyn Fn() -> Result<Box<dyn LayoutItem>, LayoutError>>`)
  rather than a built widget, so a menu can be opened twice.
- `telar::Theme<T>` is new: a zero-sized handle whose `get()` reads the theme in force. The generated view
  binds one as `theme`.
- `BorderRadius` gains `From<f32>`.

### Tooling

- **Attribute-key completion on a component** now answers from its props struct — names, types and doc
  comments, through the embedded rust-analyzer — where before it offered nothing at all.
- **Renaming a component** rewrites the module segment of every `use` line that imports it, which a rename
  used to leave pointing at a module that no longer existed.
- `cargo telar check` maps every value's span, so `fill:$theme.nonsuch` reports the theme's real fields on
  the `.rsx` line that holds it.
