# telar-components

First-party widget catalogue for Telar: buttons, fields, selects, tabs, sliders, menus, modals, accordions
and more.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**An application depends on this crate directly**, alongside the [`telar`](https://crates.io/crates/telar)
facade. The facade carries the primitives every widget is built from and no widgets of its own; this crate
is the batteries, written against the same public API an application's own components use. Every widget is
in every build, and an application links only the ones it calls; a feature brings in a dependency, and none is
on by default:

- `icons`: draws `icon:"set:name"` ids through `telar-icons`, baked with the application. Without it, icon
  buttons and icon tabs take the same ids, a `telar::IconName` in every build, and draw a Unicode stand-in
  read from the icon's name
- `async-assets`: lets an `image` follow an `AssetLoader`, whose load state converts into what the image shows

```toml
# Cargo.toml
telar-components = { version = "0.2.2", features = ["icons"] }
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
