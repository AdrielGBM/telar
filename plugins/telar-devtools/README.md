# telar-devtools

The in-app devtools overlay `cargo telar dev` draws over a running Telar application: an FPS badge and panel, a component inspector and the build-error banner.

It is built from `telar-components` in the workbench theme, on a window-sized surface of its own, so it never takes the application's theme, direction or control size, and it survives every hot reload and failed build. Ctrl+Shift+D opens the panel (frame rate, frame time, component count and the renderer drawing), Ctrl+Shift+I the inspector, and Ctrl+Shift+B switches the renderer; the application hears those chords too. The overlay takes the pointer only over its own panels and the keyboard only while one of them has focus.

An application declares it as an optional dependency, which `cargo telar new` writes for you:

```sh
cargo add telar-devtools --optional
```

`cargo telar dev` turns that feature on, and `telar::app!` installs the overlay only under it, so the overlay is in a dev session and in nothing else. `cargo telar dev --devtools off` leaves it out of the build, and `TELAR_DEVTOOLS=0` turns off whichever overlay is installed at run time.

`telar` itself never depends on this crate. It implements [`ui_tree::DevOverlay`], which is the whole of what the runner asks of an overlay, and yours goes in through the same door:

```rust
telar::run_app_with_devtools::<MyApp, MyOverlay>(config, app, "my-app");
```

[`ui_tree::DevOverlay`]: https://docs.rs/telar-ui-tree/latest/telar_ui_tree/trait.DevOverlay.html

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
