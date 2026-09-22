# Surface size and breakpoints

Every tree is drawn on a surface: a window, a browser page, a terminal, an offscreen texture. A Telar
application can read how big that surface is, write lengths as a fraction of it, and choose values by its
width. All three work the same way on every target.

```rust
use telar::{breakpoint, use_surface_width};

let wide = use_surface_width() >= 1024.0;
let gutter = breakpoint(8.0).at(640.0, 16.0).at(1024.0, 28.0).follow();
```

```rsx
[view]
col pad:$gutter
    box width:50sw max_width:100% height:10sh
```

## Reading the size

The size is in the logical units the app's layout is written in, the same units as `width:120`. Each side is
its own signal, so code that reads only the width does not re-run when only the height changes. On a phone
the browser toolbar changes the height while you scroll, and the width stays the same.

| Function | Reactive | Returns |
| --- | --- | --- |
| `use_surface_size()` | yes, both sides | `Size` |
| `use_surface_width()` | yes | `f32` |
| `use_surface_height()` | yes | `f32` |
| `surface_size()` | no | `Size`, for event handlers |

The runner sets the size before it builds the tree, so the first build already has the real size. Then it
updates the size on every resize, before the tree gets the `WindowResized` event. So the layout pass for a
resize uses the new size. `set_surface_size` writes the size. The runner calls it. A test, or a tool with no
platform, can call it too.

### More than one surface

Each surface has its own size. A component reads the size of the surface it was built on. That is also true
in its effects and memos and in the event handlers that surface dispatches, because each one runs inside its
own surface. When a side panel is resized, a component in the main window is not notified.

## Surface units

A length can be a fraction of the surface, in hundredths, like `%`. The difference is that `%` is a
fraction of the parent, and these are a fraction of the surface.

| `.rsx` | Rust | Fraction of |
| --- | --- | --- |
| `50sw` | `SizeDimension::SurfaceWidth(0.5)` | the surface's width |
| `50sh` | `SizeDimension::SurfaceHeight(0.5)` | the surface's height |
| `50smin` | `SizeDimension::SurfaceMin(0.5)` | the shorter side |
| `50smax` | `SizeDimension::SurfaceMax(0.5)` | the longer side |

They are named after the surface and not the viewport because a window and a terminal have a surface too.
You can use them anywhere a length is accepted: sizes, min and max sizes, flex basis, padding, margins, insets,
gaps and grid tracks (`cols(25sw 1fr)`, `cols(fill 20smin)`, or `TemplateTrack::length(SizeDimension::SurfaceWidth(0.25))`
in Rust).

The layout engine resolves them. When a style is applied to a node, each fraction becomes pixels for the
current size of the surface. On a terminal, those pixels are then snapped to whole cells, like any other
length. The cell a `stroke` reserves for its frame is also worked out from the resolved padding, so
padding written in surface units counts as the cells it resolves to. When the surface is resized, the engine resolves only the nodes that use these units again, and lays
them out on the next pass. Nothing in the view re-runs: `width:50sw` is a literal, so the transpiler does not
wrap it in an effect.

## Breakpoints

`breakpoint(base)` starts with the value used below every threshold. `.at(min_width, value)` adds a step that
starts at that width. A width exactly at a threshold belongs to the step that starts there, like CSS
`min-width`. You can add steps in any order. A second step at the same width replaces the first.

- `value_at(width)` and `range_at(width)` are pure functions, for tests and for code that already has a width.
- `.follow()` returns a `Memo` of the value for the current surface. It is resolved again only when the width
  moves into a different range. A resize inside a range does not re-run anything that reads the memo. Two
  neighbouring ranges with the same value do not count as a change either.

In `.rsx`, a breakpoint is ordinary Rust in `[logic]`, and you read it with `$`:

```rsx
[logic]
let gutter = breakpoint(8.0f32).at(640.0, 16.0).at(1024.0, 28.0).follow();
let tile = breakpoint(SizeDimension::Percent(1.0)).at(640.0, SizeDimension::Percent(0.48)).follow();

[view]
box pad:$gutter
    box width:$tile height:40
```

A layout value that is not a literal is already resolved again whenever what it reads changes. That is the
`style_follows` mechanism described in [animations.md](animations.md). So a breakpoint needs no new syntax
and no registry entry. It also has the same cost as a theme switch: one relayout when a threshold is
crossed, and none for each pixel of the resize.

## Where each target gets its size

| Target | Source | Units |
| --- | --- | --- |
| Web, both renderers | the host element's bounding box, measured on every frame. A window `resize` requests a frame | CSS pixels, rounded |
| Desktop | the winit window's `Resized`, divided by its scale factor | logical pixels |
| Android | the same winit path | logical pixels |
| Terminal | the terminal's `Resize`, as columns × rows | whole cells × the cell size the layout uses (8 × 16 by default). `width / layout_grid().x` is the column count |
| Headless | the size the window was created with, then each size from `HeadlessPlatform::with_resizes` | logical pixels |
| `TextureUi` | the texture's size divided by its scale | logical pixels |
| Plugin (`telar-plugin`) | the area the host gives the plugin | logical pixels |

A breakpoint written once works on every target. On the default terminal grid, the width `640` is 80
columns.

**On a document, surface units are sent as pixels.** On the DOM target the browser does the layout, and it
gets the CSS the engine resolved. A surface unit is written there as the pixels it resolved to, not as
`vw`/`vh`/`dvh`. Those CSS units name the browser viewport, and that is not always the surface:

- the host can be an element smaller than the page;
- a scrollbar takes part of `100vw`;
- a phone toolbar makes `vh` and `dvh` different;
- the surface is measured in rounded pixels.

Only pixels are sure to give the same result as the layout engine. A box whose CSS contains surface units
also follows the surface size, so it writes new CSS on a resize, even when its own rectangle does not move.

## For a backend author

Send `Event::WindowResized { width, height }` in logical units whenever the surface changes size. The runner
passes it on to `set_surface_size` before the tree gets the event. A host that builds its tree outside the
runner calls `set_surface_size` itself, inside that surface's world, before building the tree and on every
resize. `TextureUi`, `telar::testing::mount`, the `telar-plugin` driver and `cargo telar test` all do this.
