# telar-devtools

The in-app devtools overlay `cargo telar dev` draws over a running Telar application: an FPS badge and panel, the inspection panels and the build-error banner.

It is built from `telar-components` in the workbench theme, on a window-sized surface of its own, so it never takes the application's theme, direction or control size, and it survives every hot reload and failed build. Ctrl+Shift+D opens the panel (frame rate, frame time, component count and the renderer drawing), Ctrl+Shift+I the inspector, and Ctrl+Shift+B switches the renderer; the application hears those chords too.

The inspector reads the whole window, a hot-reloaded application included. **Components** lists what is mounted, as a tree, over the selected component's position, size and box model, which it marks over the application in bands for its margin, border, padding and content; **Outline** draws every component's box and **Measure** the selected one's size and its distance to the box around it. **Accessibility** lists what the accessibility checks found and what a screen reader is told, in reading order, and marks the row chosen. A surface shown inside the window, such as a workshop canvas, is read in its own world and placed where the window shows it.

The panels are public, so the component workshop mounts the same ones over its canvas: an [`Inspection`] reads an [`InspectSource`], the whole window or one canvas, and `inspector_panel`, `accessibility_panel`, `inspection_toggles` and `inspection_marks` show it. The overlay takes the pointer only over its own panels and the keyboard only while one of them has focus.

An application declares it as an optional dependency, which `cargo telar new` writes for you:

```sh
cargo add telar-devtools --optional
```

`cargo telar dev` turns that feature on, and `telar::app!` installs the overlay only under it, so the overlay is in a dev session and in nothing else. `cargo telar dev --devtools off` leaves it out of the build, and `TELAR_DEVTOOLS=0` turns off whichever overlay is installed at run time.

`telar` itself never depends on this crate. It implements [`ui_tree::DevOverlay`], which is the whole of what the runner asks of an overlay, and yours goes in through the same door:

```rust
telar::run_app_with_devtools::<MyApp, MyOverlay>(config, app, "my-app");
```

: https://docs.rs/telar-devtools/latest/telar_devtools/struct.Inspection.html
[`InspectSource`]: https://docs.rs/telar-devtools/latest/telar_devtools/trait.InspectSource.html

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
