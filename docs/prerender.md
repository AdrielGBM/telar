# Prerendering

`cargo telar build --target web --prerender` writes every page of a browser build ahead of time: the
document the browser would first show at each location, in each locale, readable before the module has
loaded — by a person on a slow connection, a reader with scripts off, and a crawler. Each page also carries
the inputs it was built from, so a client can start from the same ones.

Prerendering is web packaging, so it lives in the CLI and in `[telar.web.prerender]`, never in `.rsx`. The
one thing an application says about it is which pages it has, and that is a property of its routes that
means the same on every target.

## Using it

```sh
cargo telar build --target web --renderer dom --prerender
```

```toml
[telar.web.prerender]
width = 1280            # the surface a page is laid out on, in CSS pixels; default shown
height = 800            # default shown
color_scheme = "light"  # "light" or "dark"; left out, unknown
reduced_motion = false  # left out, unknown
high_contrast = false   # left out, unknown
locales = ["en"]        # the system locales assumed, most preferred first; left out, none
```

Every key is optional. A value left out is one the page is built not knowing, which is what a browser that
has not answered yet reports too. The table is inherited from a workspace `telar.toml` key by key.

The pages an app has are the ones its route type lists in `Route::pages`, while a navigator built on it
follows the app's address:

```rust
impl Route for Page {
    // to_location, from_location, title …

    fn pages() -> Vec<Self> {
        vec![Page::Home, Page::Projects, Page::Credits]
    }
}
```

Leave out a route that needs something only the running app has — a search result, a record fetched at run
time. It is still opened by its address; it is just not written ahead. An app whose routes name no pages, or
that follows no address, gets its root page alone.

## What the build writes

| File | What it is |
| --- | --- |
| `index.html` | The app at its root. Under an address that carries a locale, the root in the locale the app settled on. |
| `<locale>/<segments…>/index.html` | Every page in every locale the address carries (`en/index.html`, `es/projects/index.html`); `<segments…>/index.html` when it carries none. |
| `404.html` | What the app shows at an address it has no page for. Its URLs are written from the site's root, since a host serves it at any depth. |

Each is the page template expanded as for a plain build, with:

- `%telar.prerendered%`: the content of the element the app fills, as the elements the document renderer
  would create, each carrying `data-telar-id`;
- `%telar.host%`: the attributes of that element — `data-telar`, the surface's background and color scheme,
  and the document-scroll overrides when the page's root scrolls as the document;
- `%telar.state%`: the inputs, as `<script type="application/json" id="telar-state">`;
- `%telar.meta%` followed by the stylesheet the boxes need to look as they will once the app runs;
- `%telar.lang%`, `%telar.dir%` and `%telar.title%` from the page itself: its locale and the surface title the
  app derived for it (see [docs/surface-title.md](surface-title.md)).

Every URL a page writes for an output file is relative to the page (`../../app-….js` two directories down), so
the site works from any directory on any static host.

## How a page is written

The build compiles the app once more, for the machine running it, with the frontend the web build names
and `telar/prerender`, then starts that binary once per page with `TELAR_PRERENDER` holding the request.
The binary's `run()` sees the request and, instead of opening a window, builds the tree the way every
runner does, lets it settle and writes back what it drew. One process per page, because one browser tab is
one fresh start, and so is the client that will take the page over.

A page is written from the frame the app **settled** on: frames are run on the runner's own clock until two
in a row compose the same commands and nothing animates, ten seconds of motion at most. An entrance animation
has finished, so content that fades in is written visible. A page still changing after that is written as it
stands, with a warning naming it.

Text is measured with the glyph shaper and the app's own fonts, where the browser measures with its own
canvas. The elements carry what each box *declared* (its flex and grid intent), not the rects measured here,
so the browser lays the page out itself and a difference in measuring moves nothing — except where the app
itself branches on a measured size.

## One document, two writers

What a frame says the document is — which elements, in which order, with which attributes, CSS and SVG — is
worked out once, in `telar-renderer-dom`'s `document.rs`. The browser's reconcile brings the live document in
line with it; the prerender writes it out as markup (`html.rs`). Both read the same description, built by the
same functions, so a page written ahead of time and the page the app then draws are the same elements with
the same attributes. `src/prerender_parity_test.rs` checks it in a real browser: one frame, reconciled into one
host and written as markup into another, read back as the same document.

## The determinism contract

A prerendered page names every element by its `data-telar-id`, which is the layout node the widget was built
with. Layout nodes are numbered in the order they are created, so the same tree built in the same order from
the same inputs carries the same ids — on the host that wrote the page and in the browser that loads it.

**The inputs** are what the page carries in `telar-state`:

| Field | What it is |
| --- | --- |
| `version` | The format; a client reading another builds from nothing. |
| `location` | The address the app settled on, with no base path (`/es/projects`). `null` on `404.html`. |
| `locale` | The locale the app was in. |
| `preferences` | The system preferences assumed (`[telar.web.prerender]`). |
| `surface` | The size the page was laid out at. |
| `signals` | The value of every keyed signal the tree read (`hot_signal`), and the theme's mode and scheme preference, as JSON by key. |

**The order** is the runner's. Before the tree is built: the assumed preferences are reported, the surface
size, the location, and the surface title the window was opened with. Then the app's `root()` is mounted and
told the surface's size. A client that does the same, from the same inputs, builds the same nodes.
`crates/telar/src/prerender_flow_test.rs` holds the two to it: the runner on a headless platform and the
prerender, given the same inputs, open the same element ids in the same order, and two prerenders of one page
are identical.

**What the app owes it:** a tree built from those inputs alone. Anything else the first build reads — the
clock, a random number, the machine's environment, a file — gives the client a different tree. State that
does not come from the source has to be keyed (`hot_signal("…", init)`) to travel in the page; a `.rsx`
signal starts from its declared value on both sides, so it needs nothing.

## Every target

Prerendering means something only to a browser, and a page is written once whatever renders it later:

| Target | What happens |
| --- | --- |
| web-dom | The page is the document the reconcile would build. Until the client adopts it in place, its first frame stands in the served content's place. |
| web canvas | The page is readable until the canvas draws its first frame, which then replaces it. On a browser that cannot draw, it stays. |
| desktop, TUI, Android, headless | Not applicable: there is no page to serve. `Route::pages` means the same there — the places an app has — for any tool that visits them. |

## Limits

- `404.html` writes its URLs from `/`, which is right for a site at the root of its domain; a site served
  under a path needs that path as its base, which `[telar.web]` does not name yet.
- Under an address that carries a locale, `/` holds the root page in the locale the app chose; choosing the
  reader's on arrival is the page that will negotiate it.
- The client does not adopt the prerendered elements yet: it builds its own and they replace the served ones
  in its first frame, which is the same document, so nothing visible moves.
