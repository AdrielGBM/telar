# telar-devtools

The in-app devtools overlay `cargo telar dev` draws over a running Telar application: an FPS counter, a live node inspector and the build-error banner.

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
