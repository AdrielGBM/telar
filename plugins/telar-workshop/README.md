# telar-workshop

The component workshop `cargo telar preview` opens: every preview an application carries, listed in a sidebar, rendered one at a time in a canvas of its own, with panels to edit its args live.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

An application declares it as an optional dependency, beside `telar-devtools`, which `cargo telar new` writes for you:

```sh
cargo add telar-workshop --optional
```

`cargo telar preview` turns that feature on, and `telar::app!` starts `WorkshopApp` only under it, so the workshop is in a preview build and in nothing else. Without the dependency, `cargo telar preview` falls back to the plain preview page.

The workshop runs in the same runtime as the previews it shows, so a hot reload keeps the selected preview, its args and the pane sizes. Each preview renders in a canvas with a surface of its own: its overlays, focus and size stay inside the canvas, and it draws in the application's theme while the workshop's own chrome draws in the workbench theme `telar-devtools` defines.

The preview to open on comes from the request `telar::preview::host::requested_preview` reads: `TELAR_PREVIEW_ID` names it, and `TELAR_PREVIEW_COMPONENT`, which `cargo telar preview --component <name>` sets, selects the first preview of that component, matched by its component name or the last segment of its title. The workshop still lists every preview; the request only chooses where it opens. A selection restored after a hot reload wins over the request, and with no request the first preview is selected.

`telar` itself never depends on this crate. It reads `telar::preview` and builds its chrome from `telar-components`, the same public API an application's own code uses.

## Strings

The workshop's own text is in English, under the `telar_workshop` namespace. An application translates or overrides any of it by adding `telar_workshop.<key>` to its own catalog.

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
