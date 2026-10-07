# telar-watch

Filesystem watching for Telar, delivered on the UI thread.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**An application depends on this crate directly**, alongside the [`telar`](https://crates.io/crates/telar)
facade, which carries the seam it is written against (`spawn_stream`) and no file-notification backend of
its own.

```toml
# Cargo.toml
telar-watch = "0.2.1"
```

```rust
let _watch = telar_watch::watch_path(config_dir, move || settings.set(load_settings()));
```

`watch_path` calls back on **the thread that asks for it** whenever a file, or a directory and everything
under it, changes. Events are coalesced, so one editor save is one call, and a read of a watched file is not a
change. The returned `Task` owns the watch: drop or cancel it to stop.

It ships no `.rsx` tags, so there is nothing to add to `telar.toml`.

Keep it on the same version as `telar` — the same lockstep `telar` and `telar-macros` already have, and for
the same reason.

- API documentation: <https://docs.rs/telar-watch>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
