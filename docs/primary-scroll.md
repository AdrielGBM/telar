# Primary scroll

A surface has at most one **primary scroll**: the scroll that stands for the whole page. A root `ScrollPage`
is it. Every target keeps the same offset, reveal and sticky behaviour for it; the difference is who moves
the content.

```rust
use telar::ScrollPage;

let page = ScrollPage::new_with(|viewport| {
    // `viewport` is the page's ScrollViewport: offset, rect, reveal, scroll_to.
    build_content(viewport)
})?;
let viewport = page.viewport();
viewport.scroll_to(0.0, 480.0);
```

`ScrollPage::new(content)` is the same without the builder. The page's `ScrollViewport` is what nested code
reads for sticky boxes (see [docs/sticky.md](sticky.md)), and `reveal`, `scroll_to_top` and `scroll_to` move
it on every target.

## Targets

| Target | The primary scroll is |
| --- | --- |
| web-dom | The document's own scroll. The host grows with the content, the browser scrolls the page and shows its own scrollbar, and each `window` `scroll` becomes the page's `BoxScrolled`. `reveal`, `scroll_to` and `scroll_to_top` go through `window.scrollTo`. |
| Web canvas | A scroll area drawn inside a canvas the size of the viewport, as before. The canvas is a GPU surface and cannot be as tall as the content, so the page stays still and the app scrolls by drawing. |
| Desktop, Android, TUI, headless | A scroll area drawn at the offset inside the window, as before. |

### web-dom in detail

- **The host.** While a primary scroll holds the document, the document backend sets the host to
  `height: auto` and `touch-action: pan-x pan-y`, marks it with `data-telar-document-scroll`, and gives the
  page's box a `min-height` of one surface. Without `height: auto` the page cannot grow; without the
  `touch-action` a finger cannot pan it, because a touch pans only when every box between it and the scroller
  allows it. A frame without a primary scroll gives the host back as it was.
- **The surface size.** The surface stays the viewport, not the grown host: its height is the layout
  viewport (`document.documentElement.clientHeight`), which holds still while a mobile address bar slides
  away, so scrolling never lays the page out again. `use_surface_height()` and `sh` units keep meaning one
  screen (see [docs/surface-size.md](surface-size.md)).
- **Scrollbars.** The page shows the browser's own bar, and Telar draws none for the primary scroll there.
- **Pointers.** A point on the screen maps to the same surface point as before; the page's offset is what
  moves the content under it.
- **Boxes placed against the surface.** Overlays and other layout roots besides the page are placed with
  `position: fixed`, so they stay put while the page scrolls under them.
- **Before the app loads.** The built-in page leaves the document free to scroll, so a page scrolled before
  the module arrives keeps its position. The first frame reports it to the app, and puts the document back
  there if replacing the host's earlier content moved it.
- **Nesting.** Only a primary scroll at the top level of the frame, and only the first one, takes the
  document. Anywhere else it scrolls itself like any `LayoutScrollArea`, without a bar of Telar's.
- **Page position.** The document's offset is used as is, which assumes the host starts at the top of the
  document, as it does in the built-in page. Content above the host shifts what the app thinks is visible by
  that much; hit-testing stays correct.

## For platform code

`platform_core` exposes the primary scroll without knowing which widget owns it:

| Function | Does |
| --- | --- |
| `primary_scroll_offset()` | The offset, or `None` when no page claimed it |
| `scroll_primary_to(x, y)` | Moves it, and asks for a frame; `false` when no page claimed it |
| `claim_primary_scroll(scroll)` | What `ScrollPage` calls. The newest claim wins, and dropping an older one leaves it |

The web location adapter keeps the page's position per history entry through these (see
[docs/location.md](location.md)). Where the document scrolls the page it reads and moves the document's
scroll directly, since the app hears of a scroll one frame late.
