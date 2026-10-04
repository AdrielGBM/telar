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

## Reading it from anywhere

`use_scroll_viewport()` answers only while a scroll area's content is being built. `use_primary_scroll()`
answers from anywhere on the surface: the page's `ScrollViewport` while a `ScrollPage` holds the primary
scroll, `None` while none does. It is reactive, so whoever reads it runs again when a page takes the primary
scroll or lets it go (the newest page wins, as with the platform's claim below), and the viewport's offset,
rect and `progress` are signals of their own. A bar fixed over the page (see
[docs/fixed-layer.md](fixed-layer.md)) follows the page's scroll this way:

```rsx
[logic]
let read = memo(|| use_primary_scroll().map_or(0.0, |page| page.progress(Axis::Vertical)));
```

## The place under a line

`use_anchor_at(line)` is the anchor whose box spans the line `line` px below the top edge of the primary
scroll's view: the name `anchor:` gave it, or `None` while no page holds the primary scroll or no place on it
spans the line. A bar fixed over the page reads it with its own height to know which section is under it, to
mark the link to that section current (see [docs/links.md](links.md#the-current-link)) and to take that
section's look:

```rsx
[logic]
let here = memo(|| use_anchor_at(48.0));
let act = memo(move || act_of(here.get().as_deref()));

[view]
layer
    row theme:(theme_for($act)) width:100% height:48
        box to:anchor("web") current:($here.as_deref() == Some("web"))
            text "Web"
```

- **Reactive.** Whoever reads it runs again as the page scrolls, as places come and go or are renamed, as
  their boxes move and when a page takes the primary scroll or lets it go. A `memo` over it changes only
  when the place does, so a bar re-dresses itself once per section rather than once per scroll tick.
- **Which place.** A box spans the line from its top edge to just above its bottom edge, so of two sections
  that meet at the line the one below it is under it. Where several places span it, the one whose top the
  reader passed last wins, and of two whose tops are level the shorter: the innermost of nested places. A
  place inside a scroll area of its own counts only where that area shows it; a place outside the page's
  scroll, in a layer or an overlay, never does. Only where a box runs top to bottom counts: the line
  crosses the whole width of the view.
- **Any scroll.** `ScrollViewport::anchor_at(line)` asks the same of any scroll: a sticky bar inside a
  scroll area reads its own area with `use_scroll_viewport()`, as the sandbox's *Current place* page does.
- **Every target alike.** It is worked out from Telar's layout and the scroll's offset, as `scroll_progress`
  is: on web-dom the offset is the document's scroll, which the page hears as `Event::BoxScrolled` one frame
  after the browser moved it; everywhere else it is the scroll area's own offset. Headless and the terminal
  answer the same, on their own layout.

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
  `position: fixed`, so they stay put while the page scrolls under them. A layer fixed over the page is
  `position: fixed` too, but its element stays inside the page where it was declared, so Tab and a reader
  reach it in that order (see [docs/fixed-layer.md](fixed-layer.md)).
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
