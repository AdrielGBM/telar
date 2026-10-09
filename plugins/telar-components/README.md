# telar-components

First-party widget catalogue for Telar: buttons, fields, selects, tabs, sliders, menus, modals, accordions
and more.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**An application depends on this crate directly**, alongside the [`telar`](https://crates.io/crates/telar)
facade. The facade carries the primitives every widget is built from and no widgets of its own; this crate
is the batteries, written against the same public API an application's own components use. Nothing is on by
default, and the buttons, fields, selects, tabs, sliders and indicators are in every build. Name the groups
you draw beyond them:

- `overlays`: menus, context menus, modals, drawers, tooltips, the command palette and the key caps its rows
  show, and toasts
- `chrome`: the window frame a desktop application draws itself
- `advanced`: reorderable lists, accordions and steppers
- `workbench`: split panes, trees, toolbars, icon buttons, a colour picker and a code view, with the `icons`
  and `overlays` they build on

```toml
# Cargo.toml
telar-components = { version = "0.2.2", features = ["overlays"] }
```

Then list it in the app's `telar.toml`, so every `.rsx` file sees its components without a `use`:

```toml
[telar]
prelude = ["telar_components"]
```

Keep it on the same version as `telar` — the same lockstep `telar` and `telar-macros` already have, and for
the same reason.

- API documentation: <https://docs.rs/telar-components>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
