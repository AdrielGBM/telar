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
| `index.html` | The app at its root. Under an address that carries a locale, the page that sends a reader to the root of their locale instead; see [the root of a site in several locales](web-packaging.md#the-root-of-a-site-in-several-locales). |
| `<locale>/<segments…>/index.html` | Every page in every locale the address carries (`en/index.html`, `es/projects/index.html`); `<segments…>/index.html` when it carries none. |
| `404.html` | What the app shows at an address it has no page for. Its URLs are written from the site's root, since a host serves it at any depth. |

Each is the page template expanded as for a plain build, with:

- `%telar.prerendered%`: the content of the element the app fills, as the elements the document renderer
  would create, each carrying `data-telar-id`;
- `%telar.host%`: the attributes of that element — `data-telar`, the surface's background and color scheme,
  and the document-scroll overrides when the page's root scrolls as the document;
- `%telar.state%`: the inputs, as `<script type="application/json" id="telar-state">`;
- `%telar.meta%`: the tags that describe the page (its description, canonical and alternate-language links,
  link-preview tags; see [Describing the site](web-packaging.md#describing-the-site)), followed by the
  stylesheet the boxes need to look as they will once the app runs;
- `%telar.lang%`, `%telar.dir%` and `%telar.title%` from the page itself: its locale and the surface title the
  app derived for it (see [docs/surface-title.md](surface-title.md)).

Every URL a page writes for an output file starts with the site's base path (`/app-….js`, `/docs/app-….js`
under `base = "/docs/"`), and so do the links and pictures inside it: the app is prerendered with that base as
the path its addresses and its files live under, the same one it reads from the page once it runs. With
`--prerender` the build also writes `sitemap.xml` when the site has an `origin`.

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

## Taking the page over

Under the document renderer the client **adopts** the page it was served instead of building it again: the
elements the reader is already looking at, scrolling and maybe focused on are the ones the app goes on
drawing into. A web canvas does not: it replaces the page once it has drawn its first frame.

### Starting from the page's inputs

Before the tree is built, the runner reads `telar-state` and starts from what the page was written from:

1. the **assumed preferences** are reported as the system's, the **surface** is the size the page was laid
   out at, the **locale** is the page's, and every **keyed signal** and the theme's mode and scheme preference
   are restored, so `hot_signal("…", init)` starts from the page's value rather than from `init`;
2. the location is the address the page is served at, which is the one it was written for, and the tree is
   mounted;
3. then the browser catches up, the way any later change arrives: the preferences it really reports, the real
   surface size and the scheme the reader chose (`telar.scheme` in the preference store);
4. every animation the build started is run to its end, because the page was written once the app had
   settled: an entrance has already played for the reader, and playing it again would hide what they are
   reading.

Ids are handed out by a counter, so the order matters more than it looks. A tree built straight from this
browser's preferences shifts the id of every node after the first one that differs; built from the page's
inputs and then updated, only the subtree that really changes is new. A state of another `version`, or one
that cannot be read, is ignored, and the app builds from nothing as it does without a page.

### The first frame

The document renderer indexes the host's elements by `data-telar-id` when it is created, and its first frame
takes each box's element from there instead of creating one:

- **Matched by id, checked by tag and parent.** An element whose tag is the box's, and whose parent is the
  element the box belongs in, is kept. Its style, attributes, text and drawing are read once and compared with
  what the frame says, and what differs is **patched in place**: an attribute set or removed, text rewritten in
  its own text node, a shape's attributes changed. A selection inside a paragraph survives a translated word.
- **Rebuilt only where it no longer fits.** An element served with another tag, or under another parent, is
  not that box any more. Its subtree is built from the frame, with a warning in a debug build and a `tracing`
  event in release, since it means the page was written from inputs the client does not share. Its parent and
  siblings are still adopted: the host is never rebuilt whole.
- **Paint that is not a box** (the host's own panels, the presentation pieces inside a box) is adopted in
  order, the same way.
- **Swept once the frame is done.** Nothing served is removed while the frame may still claim it; what it did
  not claim goes at the end.
- **The text entry**, the `<textarea>` the document renderer types through, is not part of the page. It is
  appended after the served content, which moves nothing, or taken over if the host already holds one.

A drawing over pixels the app makes at run time is written without them: the `<image>` is in the page with no
address, and the first frame gives it one, which is an attribute rather than a new element. The files a page
links (a picture's `src` and `srcset`) are addressed by the running app from the site's root whenever they
are on the page's own origin, the way a page writes them, so an adopted `<img>` keeps the address it was
served with.

### Continuity

What the reader did before the module loaded stays theirs:

- **Scroll.** The document keeps its scroll; the primary scroll reports where it arrived, and the arrival is
  kept through the first frame. A box that scrolls itself keeps its offset, and the app is told it.
- **Focus.** A reader who tabbed to a link keeps it: the platform does not take focus for the app when
  something on the page already holds it, and the first frame tells the app which box that is.
- **Selection and media.** The text nodes and the `<img>` elements are the served ones, so a selection and a
  picture already decoded stay as they are.
- **What happens before the app runs.** Links are real `<a href>` and the page scrolls as the document, so both
  work with no module at all; the app takes the clicks back once it runs.

When the primary scroll stops being the document's, the host is given back without the overrides the page was
served with (`height`, `touch-action`, `overflow-x`), as if the app had set them itself.

`crates/renderer/renderer-dom/src/hydration_test.rs` checks the first frame in a real browser: the same nodes
before and after, no structural mutation on a clean take-over (`MutationObserver`), attributes and text patched
where they stand, a subtree rebuilt alone, `to:` and the keyboard on adopted elements, and scroll and focus
kept. `crates/telar/src/runner/hydration_test.rs` checks the order: a tree whose shape depends on the scheme is
built under the page's and opens the page's ids, then shows this browser's scheme, the page's keyed signal and
its entrance at its end in the first frame.

## Every target

Prerendering means something only to a browser, and a page is written once whatever renders it later:

| Target | What happens |
| --- | --- |
| web-dom | The page is the document the reconcile would build, and the client takes it over in place from the inputs it carries. |
| web canvas | The page is readable until the canvas draws its first frame, which then replaces it. On a browser that cannot draw, it stays. |
| desktop, TUI, Android, headless | Not applicable: there is no page to serve. `Route::pages` means the same there — the places an app has — for any tool that visits them. |

## Limits

- The first tree is built under the page's assumed preferences and size, so a reader whose browser differs
  sees the difference arrive with the first frame (the colours of another scheme, the layout of another
  width) rather than in the served page.
- A page served at an address other than the one it was written for (`404.html` at any path, a host that
  rewrites unknown paths to a page) is built for the address it is at, and whatever differs is patched or
  rebuilt as above.
