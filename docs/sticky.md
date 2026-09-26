# Sticky positioning

A sticky box stays in the layout flow. When its scroll viewport scrolls past it, the box stays at the
distance its insets name from the viewport's edge. It never leaves its parent's content box. This is CSS
`position: sticky`, and it works the same way on every target.

```rsx
[view]
scroll height:400
    col
        for section in 0..3
            col
                box sticky inset_top:0
                    text "Section {section}"
                for i in 0..20
                    text "Row {i}"
        box sticky inset_bottom:0
            text "Footer"
```

```rust
LayoutStyle::new().sticky().inset_top(0.0)
```

## Attributes

| `.rsx` | Rust | Meaning |
| --- | --- | --- |
| `sticky` | `LayoutStyle::sticky()` | Makes the box sticky. `absolute` and `absolute:fill` turn it off again, and the last one written wins. |
| `inset_top:N` | `.inset_top(N)` | Stays at least `N` below the viewport's top edge. |
| `inset_bottom:N` | `.inset_bottom(N)` | Stays at least `N` above the viewport's bottom edge. |
| `inset_start:N` / `inset_end:N` | `.inset_start(N)` / `.inset_end(N)` | Same on the inline axis. They follow the writing direction, like every logical edge. |

An edge with no inset does not stick. A percentage is a fraction of the viewport on the same axis, as CSS
resolves it against the scrollport. A fraction of the surface (`10sh`) is converted to pixels like any other
length.

## How it is placed

Layout places a sticky box exactly where it would place an ordinary box. After layout, a separate pass moves
it using the view: the part of the content the viewport shows, in the content's own coordinates.

- **The view.** The nearest `scroll` above the box (a `LayoutScrollArea`, which includes the one inside
  `ScrollPage`) sets the view for its content every time the offset or the viewport changes, through
  `layout_reactive::set_sticky_view`. A layout root that nothing set a view for sticks against its own box.
  That box never scrolls, so a sticky box outside any scroll stays in place, the same as in a document that
  does not scroll.
- **The rule.** The box moves as far as it needs to so that its border box stays the named distance inside
  the view. It moves no further than it can while its margin box stays inside its parent's content box, which
  is its containing block. When both edges of an axis stick and the box cannot satisfy both, the top edge and
  the left edge win.
- **The subtree.** The box's descendants move with it. A sticky box inside another sticky box is placed
  relative to where its parent was moved to.
- **The cost.** A scroll does not run layout again. The runtime keeps the outermost sticky boxes of each root
  from its last walk and, when the view changes, walks only their subtrees again (`LayoutEngine::walk_anchor`).
  The rest of the content does not move, so it is not visited.

The moved rect is what a node's rect signal publishes (`track_layout`), so hit-testing, `visible_rect`,
anchored overlays and the paint of every backend all read the same position.

## Targets

| Target | How it sticks |
| --- | --- |
| web-dom | The box also gets native `position: sticky` with the same insets, so the compositor holds it between frames. The rect Telar computes is still what hit-testing reads, and the layout parity test (`crates/renderer/renderer-dom/src/layout_parity_test.rs`) checks that both agree in Firefox. A `clip` becomes `overflow: clip`, not `overflow: hidden`, because a `hidden` box is a scroll container and a sticky box inside it would stick to that box instead of to the viewport. |
| GPU, software, web canvas, Android | The moved rect is drawn directly. |
| TUI | The same, on the cell grid. The offset is snapped the same way the scroll offset is and the insets are snapped like every inset, so the box sticks in whole cells and lines up exactly with the viewport's edge. |
| headless | The moved rect is published, so tests read where the box is stuck. |

## Limits

- A layout root is never sticky itself, because it has no containing block to stay inside.
- A box wrapped in `animate_layout` measures its position against its parent. A sticky box
  changes that position on every scroll, so the transition animates the sticking. Do not use both on the
  same box.
