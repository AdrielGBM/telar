# Dashed strokes

A stroke can be drawn in dashes: a pattern of lengths that alternate drawn and skipped along the outline,
starting with a drawn one, and an offset that says how far into the pattern the stroke begins. `1 4` is a
dot every five units; `8 4` is a dashed line; `10 4 2 4` is dash-dot.

```rsx
path d:"M0,0 L100,0 L50,80 Z" stroke:$theme.ink stroke_width:1 stroke_dash:"1 4"
path d:"M0,40 L120,40" stroke:$theme.primary stroke_width:2 stroke_dash:"8 4" stroke_dash_offset:4
```

```rust
Stroke::new(color, 1.0).with_dash(&[1.0, 4.0], 0.0)
```

In a `canvas` the stroke goes on a path's style like any other, so one path holding many edges dashes all of
them with one call:

```rust
RenderNode::path(Arc::new(edges), PathStyle::default().with_stroke(
    Stroke::new(ink, 1.0).with_dash(&[1.0, 4.0], 0.0),
))
```

## The pattern

The pattern is a [`Dash`](https://docs.rs/telar/latest/telar/struct.Dash.html) on the stroke, normalised when it
is built so every target reads the same one:

- An odd list is repeated into an even one, as SVG and canvas do: `3` is `3 3`, `1 2 3` is `1 2 3 1 2 3`.
- The offset may be negative or larger than the pattern; it is wrapped into one period. A positive offset
  shifts the dashes back along the stroke.
- A list that draws no dashes — empty, a negative or non-finite length, every length zero, or more than
  sixteen lengths once repeated — leaves the stroke solid, which is what SVG and canvas draw for it too. In
  `.rsx` the same list is a build error on the attribute instead, so it cannot go solid without saying so.
- The pattern restarts at every subpath. On a closed subpath the dash that runs through its start is one
  dash joined there, not two capped ones.
- A drawn length of zero is a dash with no length: with `cap: Round` or `Square` it is a dot, so
  `stroke_dash:"0 6"` on a round-capped stroke is a dotted line.
- Lengths are in the same units as the stroke's width, and scale with it: a transform or a device scale
  factor scales the pattern along with the path.

`stroke_dash` takes lengths separated by spaces or commas, as SVG's `stroke-dasharray` does.
`stroke_dash_offset` takes a plain number and needs a `stroke_dash`, which in turn needs a `stroke`. Both are
literals; a pattern that changes with state belongs in a `canvas`.

An SVG asset's own `stroke-dasharray` and `stroke-dashoffset` are kept as a dashed stroke too, so a dashed icon
stays vector instead of falling back to a raster.

## Per target

| Target | A dashed stroke is |
| --- | --- |
| GPU | Cut into its dashes before lyon tessellates it: the path is flattened to the tessellator's tolerance and walked by the pattern in `renderer-core` ([`Dash::split`](https://docs.rs/telar-renderer-core/latest/telar_renderer_core/struct.Dash.html#method.split)), and each dash is stroked with the stroke's own caps and joins. A line is cut by the same walk into one instance per dash. The tessellated dashes are cached with the pattern as part of the key. |
| Software | `tiny_skia::StrokeDash`, so the rasterizer dashes it while stroking. A drop shadow under a dashed path is dashed too. |
| Web, document | SVG `stroke-dasharray` and `stroke-dashoffset` on the drawing's `<path>` or `<line>`. |
| Web, canvas | As the GPU. |
| Headless | As the software target. |
| Terminal | A cell is far coarser than any pattern, so the run is drawn in the dashed line characters instead of being cut: `┈` `┊` when the pattern draws less than it skips, `╌` `╎` otherwise. Unicode has no dashed diagonals, so a slanted run stays `╲` `╱`. |

## Limits

- The pattern holds at most sixteen lengths.
- A box's border is a frame rather than a stroke and is always solid.
- On the GPU a curve is cut into straight dashes at the tessellator's tolerance in the path's own units, so a
  path drawn under a large scale shows the facets the same way its solid stroke would.
