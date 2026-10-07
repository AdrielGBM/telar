# telar-navigate

Page-stack navigation for Telar: a reactive navigator and an animated navigation host.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

**An application depends on this crate directly**, alongside the [`telar`](https://crates.io/crates/telar)
facade. The facade carries the mechanism every navigator stands on — the app's address, its history on
every target, `to:` links, and the seam a route stack follows the address through — and no route stack of
its own; this crate is one, written against the same public API an application's own code uses.

```toml
# Cargo.toml
telar-navigate = "0.2.1"
```

Then list it in the app's `telar.toml`, so every `.rsx` file sees `Navigator` and the rest without a `use`:

```toml
[telar]
prelude = ["telar_navigate"]
```

`Navigator<R>` works with any `Clone` route type. Implement `Route` on that type — `to_location` /
`from_location` against a platform-neutral `Location` (path segments, an optional in-page-anchor fragment,
query-style params; not a URL) — and `Navigator::follow_location` makes the stack the app's history on every
target: browser history, a desktop deep link, an Android intent, a terminal argument. `Route` and `Location`
are re-exported here as the very items `telar` exports, so both preludes name one item and never clash.

A navigator that follows the app's address also names the page: the current route's `Route::title`, in the
active locale, becomes the page's part of the surface's title. See
[docs/surface-title.md](https://github.com/AdrielGBM/telar/blob/main/docs/surface-title.md).

It also declares the app's pages: the ones its route type lists in `Route::pages`, which
`cargo telar build --prerender` writes one per location and locale. See
[docs/prerender.md](https://github.com/AdrielGBM/telar/blob/main/docs/prerender.md).

`NavHost` renders the top of a stack as a page, built once and swapped with an optional `NavTransition`;
`TabStacks` and `TabHost` keep one stack per tab.

Keep it on the same version as `telar` — the same lockstep `telar` and `telar-macros` already have, and for
the same reason.

- API documentation: <https://docs.rs/telar-navigate>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>

## License

Licensed under either of [Apache License, Version 2.0](../../LICENSE-APACHE) or [MIT license](../../LICENSE-MIT) at your option.
