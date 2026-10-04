# Fixed layer

A **layer** is a set of boxes that stand against the surface instead of in the page's flow: a bar that stays
at the top of the window while the page scrolls under it, a toolbar pinned to a corner. The page never
scrolls a layer, and its sticky boxes never cover one. It is not a modal: a layer takes the pointer only over
its own boxes, and its controls keep their place in the Tab order.

```rsx
[view]
col width:100%
    layer
        row role:banner width:100% height:48 pad_x:16 align:center fill:$theme.surface
            text "Site"
            box to:anchor("about")
                text "About"
    col role:main width:100%
        col height:300sh
            box sticky inset_top:0 height:100sh
                text "stage"
```

```rust
let layer = FixedLayer::new(LayoutStyle::new().flex_column(), vec![box_item(bar)])?;
```

## How it is laid out

The `layer` itself is a box the size of the surface, at its origin, laid out as a layout root of its own
(`layout_reactive::lay_out_against_surface`). It is a column, like an `overlay`, so `align`, `justify` and
`pad` on the layer, and the sizes of its children, place its boxes: `layer justify:end align:center pad:24`
puts a toolbar at the bottom centre. Percentages and `sw`/`sh` resolve against the surface.

- **From the first frame.** The layer is laid out at the surface's size on every pass, whether or not
  anything else has been laid out yet, so a bar that is part of the first frame is in place on it.
- **Where it is declared it takes no room.** Its place in the page is a box of no size and out of the flow,
  so it adds no gap to the column it sits in.
- **Safe area.** The layer spans the whole surface. A bar that has to stay clear of a notch pads itself with
  [`use_safe_area_insets()`](surface-size.md).
- **The cascade starts over.** Like an overlay, a layer's content inherits from the surface rather than from
  the boxes around where it is declared, so a `theme:`, a `font_family:` or a `lang:` meant for it goes on
  the layer's children or inside them.
- **Scrolls.** A box inside a layer is in no scroll: `view_progress:` reads nothing there and `anchor:`
  reveals nothing. To follow the page's scroll from a layer, read [`use_primary_scroll()`](#reading-the-page-scroll-from-a-layer).

## Pointer, keyboard and stacking

- **Pointer.** A layer is hit-tested before the page, but only over its children's boxes, where they are
  drawn. A press anywhere else, the empty part of the surface the layer spans included, reaches the page under
  it. A press on a child's own area is the layer's even where no control handles it, so a click on a bar's
  background does not fall through to a link under the bar. A wheel over the layer scrolls the page.
- **Keyboard.** A layer's focusables are registered where the layer is declared, so Tab reaches them in that
  order and walks in and out of the layer freely. A layer never holds focus the way a modal does.
- **Stacking.** A layer is drawn over every box of the page, sticky ones included, and under every overlay
  (a modal, a menu, a tooltip), whatever order the two were declared in; overlays are hit-tested first for
  the same reason. Layers stack among themselves in the order they are declared. A tooltip opened from a
  control in a layer is an overlay like any other.
- **Shown by state.** A layer inside an `if` comes and goes with it, and one declared inside a box hidden
  with `shown:false` is hidden with it: not drawn, not hit, and not a Tab stop, on every target.

## Targets

| Target | A layer is |
| --- | --- |
| web-dom | An element in the document where the layer was declared, so the browser's Tab order and a screen reader's reading order are the markup's, fixed against the viewport with `position: fixed`, `z-index: 1` and `pointer-events: none`; its children take `pointer-events: auto` back, so the browser too hands it the pointer only over its boxes. The layout root it sits in becomes a stacking context (`isolation: isolate`), so overlays, placed after that root, stay over it. Checked in Firefox by `crates/renderer/renderer-dom/src/fixed_layer_test.rs`. |
| Web canvas, desktop, Android | A layout root of its own, composed after the page and before the overlays, and hit-tested before the page but only within its boxes. |
| TUI | The same, on the cell grid. |
| headless | The same; its boxes publish their rects through `track_layout`, in surface coordinates, for a test to read. |

### Limits

- **web-dom**: a layer declared inside a box that is moved (`translate_*`, `scale`, `rotate`), filtered or
  masked is placed against that box rather than the viewport, as `position: fixed` is in CSS. Declare
  layers outside such boxes; every other target draws them against the surface either way.
- **web-dom**: the viewport the layer is fixed against is where the host is, which is the top-left of the
  document in the built-in page.

## Reading the page scroll from a layer

`use_scroll_viewport()` answers only inside a scroll area's builder. `use_primary_scroll()` answers from
anywhere: the viewport of the page that holds the surface's [primary scroll](primary-scroll.md), or `None`
while no page does. It is reactive: whoever reads it runs again when a page takes the primary scroll or
lets it go, and the viewport's offset, rect and `progress` are signals of their own.

```rsx
[logic]
let read = memo(|| use_primary_scroll().map_or(0.0, |page| page.progress(Axis::Vertical)));

[view]
layer
    row width:100% height:48
        text "{($read * 100.0).round()}%"
```
