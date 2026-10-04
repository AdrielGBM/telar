# Backdrop blur

`backdrop_blur:` blurs what the surface drew beneath a box before the box paints itself, so a translucent fill
reads as frosted glass over whatever lies behind it: a header over a scrolling page, a panel over a picture.

```rsx
row sticky inset_top:0 fill:#ffffff8c backdrop_blur:20      // CSS: backdrop-filter: blur(10px)
box fill:$theme.surface backdrop_blur:$frost transition(backdrop_blur 250ms)
```

```rust
StyledContainer::new(layout, paint, children)?.with_backdrop_blur(|| 20.0)
```

The value is a radius in pixels, as `shadow_blur` is: the width of the whole falloff, where CSS's `blur()`
takes the Gaussian's deviation. CSS `backdrop-filter: blur(10px)` is `backdrop_blur:20`. A value reading `$`
state is re-read when the state changes, `transition(backdrop_blur …)` animates it, and a radius of zero or less
blurs nothing and costs nothing. Like `opacity` and `blend`, the box and everything in it become one layer.

## On each target

| Target | What it does |
| --- | --- |
| web-dom | `backdrop-filter: blur(radius / 2)` (and `-webkit-backdrop-filter`) on the box's element. The browser blurs everything the page painted behind it, up to the nearest ancestor that starts a backdrop root (one with `opacity`, a `blend:` or a mask of its own). |
| GPU (desktop, web canvas, Android) | A blur pass over the part of the frame beneath the box's rect, at the same deviation, before the box's own layer composites over it. Damage tracking repaints the whole blurred region whenever anything beneath it changes. |
| Software (headless, and desktop without a GPU) | The same, with a Gaussian blur over the pixmap beneath the box. |
| TUI | Ignored: a cell has no backdrop to blur. The fill is drawn as it is, so pick a fill that reads on its own there. |

What lies beneath the surface itself — another window, the desktop — is not this property's to blur: that is
the compositor's.
