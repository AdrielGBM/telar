# telar-ui-core

Widget kernel for Telar: containers, text, input handling, scrolling and canvas primitives.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

Focus declares what each control keeps from the keyboard. `focus::focusable_of` derives it from the role and
the control's state (disabled, hidden, inside a modal that holds focus), `StyledContainer::consumes_keys`
overrides it for state-dependent widgets, and `focus::follow_box` follows a focus move the surface made
itself. The ring shows where `focus::is_focus_visible` says, and a box that frames a control without being
one (a field's border) draws it with `StyledContainer::frames_focus_of`. See
[docs/keyboard.md](https://github.com/AdrielGBM/telar/blob/main/docs/keyboard.md).

`FixedLayer` (the `layer` tag) stands boxes against the surface over the page: a layout root of its own,
hit-tested before the page but only over its boxes, with its focusables where it was declared in the Tab order.
`use_primary_scroll` reads the page's `ScrollViewport` reactively from anywhere, and the page's arrival
margin follows the bars of its layers, so a place it is brought to lands below them. `use_anchor_at` reads which
anchor is under a line across the page's view (`ScrollViewport::anchor_at` for any scroll), and
`StyledContainer::current` marks a link as the current one of its set. See
[docs/fixed-layer.md](https://github.com/AdrielGBM/telar/blob/main/docs/fixed-layer.md) and
[docs/primary-scroll.md](https://github.com/AdrielGBM/telar/blob/main/docs/primary-scroll.md).

Each surface keeps the title it shows: `set_app_title`, `set_page_title` and `set_title_format` feed it,
`use_surface_title` reads it, and an effect owned by the surface announces each change to its platform as
`WindowCommand::SetTitle`. See [docs/surface-title.md](https://github.com/AdrielGBM/telar/blob/main/docs/surface-title.md).

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

- API documentation: <https://docs.rs/telar-ui-core>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
