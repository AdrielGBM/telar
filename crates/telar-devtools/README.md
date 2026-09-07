# telar-devtools

The in-app devtools overlay `cargo telar dev` draws over a running Telar application: an FPS counter, a live node inspector and the build-error banner.

You do not depend on this. `telar/dev` does, and `cargo telar dev` enables it — so the overlay is in a dev session and in nothing else.

It is here rather than inside `telar` for two reasons. It is four hundred lines of chrome no shipping application draws. And it implements [`ui_tree::DevPlugin`], which is the whole of what the runner asks of an overlay — a seam whose only implementation lives inside the crate that defines it is a seam nobody can be shown how to use.

Yours goes in through the same door:

```rust
telar::run_app_with_devtools::<MyApp, MyOverlay>(config, app, "my-app");
```

`TELAR_DEVTOOLS=0` turns off whichever overlay is installed.

[`ui_tree::DevPlugin`]: https://docs.rs/telar-ui-tree/latest/telar_ui_tree/trait.DevPlugin.html

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
