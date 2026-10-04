# telar-ui-tree

Component and render-node tree for Telar, with event dispatch and overlay routing.

Part of [Telar](https://github.com/AdrielGBM/telar), a modular Rust UI framework with its own template
language, reactive signals and a self-contained renderer.

A frame is composed in three strata, bottom to top: the page, the layers fixed over it (`RenderNode::fixed`)
and the overlays (`RenderNode::overlay`), so an overlay covers a layer whatever order the two were declared
in. The overlay registry hit-tests in the same order, and asks each sink `hits` rather than only its
`content_rect`, so a layer answers only over its own boxes. See
[docs/fixed-layer.md](https://github.com/AdrielGBM/telar/blob/main/docs/fixed-layer.md).

**Applications depend on the [`telar`](https://crates.io/crates/telar) facade, not on this crate.** Telar
is split into small crates so a build carries only the target and the capabilities it named, and every one
of them has to be published for the facade to be. The facade re-exports what an application needs behind
feature flags; reach for this crate directly only if you are writing a frontend or a tool against Telar's
internals.

- API documentation: <https://docs.rs/telar-ui-tree>
- The framework, and where to start: <https://github.com/AdrielGBM/telar>
