# Links

A box that takes the person somewhere is a link, whatever it looks like. Telar says where it goes once, as a
`Destination`, and each target goes there in its own way.

```rsx
box to:Page::Project(slug)            // a typed route, or a Location
box to:anchor("contact")              // a named place on the page being shown
box to:external("https://github.com") // something outside the app
```

```rust
StyledContainer::new(style, paint, children)?.to(move || Page::Project(slug.clone()))
```

`to:` takes any expression; one that reads `$state` is re-read when that state changes. `external("…")` must
name a scheme (`https:`, `mailto:`…): a literal without one is a build error on the attribute. A runtime string without one yields `None`: the box stays a link but a disabled one that goes nowhere, and a warning names the rejected string.

## What a link is

| Item | What it is |
| --- | --- |
| `Destination` | `Route(Location)`, `Anchor(name)`, `Locale(tag)` (the place being shown, in another language) or `External(Uri)`. Lives in `semantics-core`, beside `Location`, because a renderer and a platform both read it. |
| `Semantics::link` | The destination a box's element carries. `Semantics::linking_to` sets it and the `link` role together. |
| `IntoDestination` | What `to` accepts: a `Destination`, a `Location`, or any `Route`. `Route` now lives in `platform-core` (re-exported by `navigate-core` as before) so this works without the `navigate` feature. |
| `anchor`, `in_locale`, `external` | The constructors `.rsx` names. `in_locale("en")` is a language switch that keeps the page and the anchor; see [docs/location.md](location.md#the-locale-in-the-location). |
| `address_of` | A destination as the text a target writes where an address goes: a route in the app's `location_format()` and in the locale its address carries (`/en/projects`), an anchor as the current location with that fragment, an external URI as itself. |
| `follow`, `follow_beside`, `follow_pressed` | Going there from app code: push the route, reveal the anchor, open the URI. `follow_pressed` reads Ctrl, Cmd or Shift as a request for a view beside this one. |
| `open_uri`, `UriOpener`, `set_uri_opener` | The `services-core` service that hands a URI to the system. Every runner installs its platform's; headless installs none, and a test installs its own. |
| `anchor:`, `PageAnchor::page_anchor` | Names a box as a place on the page; see [Anchors](#anchors). |
| `register_anchor`, `reveal_anchor`, `has_anchor` | The lookup an anchor resolves through, which `anchor:` fills. |
| `Event::BoxActivated` | A surface that follows links itself reporting a plain activation back to the app. |

A link is a control: it joins the tab order with the `link` role, follows on a tap and on **Enter** (not Space,
which scrolls the page a link sits on), keeps no keys from a host page, and is announced with its address. An
`on_press` on the same box still runs, before the link is followed. A disabled link goes nowhere. Only `box` is a
link host in Rust (`StyledContainer::to`); a `col`, `row` or `grid` with `to:` is built as one, since a link has
to be focusable and show a focus ring.

## Anchors

```rsx
col anchor:"contact"          // this box is the place called "contact"
    …
box to:anchor("contact")      // a link to it
```

`anchor:` works on any built-in tag and takes a literal, `t!(…)` or an expression reading `$state`, so a name
can follow the locale (`#simulacion`, `#simulation`). When it does, the history follows the rename: an entry naming the old name names the new one, so a language switch keeps the reader's anchor in the address. In Rust it is `.page_anchor(|| "contact")`, which every
widget with a layout node takes through `PageAnchor`. A name belongs to one box: a second box registered under
it takes it over and a warning names it.

Following an anchor:

- **Reveals it at the start.** The box is brought to the top of the innermost scroll it sits in, and that scroll
  to the top of the next one out, up to the page. Nearest-edge scrolling is `ScrollViewport::reveal`, for
  keyboard selection; an anchor is a place, so it goes to the top the way a web fragment does. The scrolls are
  found from the box itself (`scroll_viewports_of`), so the order the tree was built in does not matter.
- **Adds a history entry.** The current page with that fragment (`/es/#contact`) is pushed, so back returns to
  where the reader was and the address can be shared. Following the anchor already shown adds nothing.
- **Keeps up with the page on arrival.** An address that names an anchor (a page loaded at `/es/#contact`, a
  deep link, a hand-edited hash) is revealed once its box is laid out, and again whenever that box moves (a face
  arriving, an image taking its size), until the reader presses, scrolls or types.

A fragment names a place on a page, never a page: the navigator following the location sees the page, and the
history keeps the anchor entries around it (see [docs/location.md](location.md)).

| Target | An anchor is |
| --- | --- |
| Web, document | The element's `id`, so `#name` is a native fragment. A plain click on its `<a>` is taken back and followed by Telar, which pushes the entry and reveals the box through the document's scroll. |
| Web, canvas, desktop, Android, terminal, headless | Revealed by Telar in its scroll areas. A desktop or terminal `--location /page#name` and an Android link with a fragment open there. |

## The current link

```rsx
[logic]
let here = memo(|| use_anchor_at(48.0));

[view]
box to:anchor("web") current:($here.as_deref() == Some("web"))
    text "Web"
```

`current:` marks a link as the current one of its set while it reads true, and is re-read whenever what it
reads changes. What it is the current one of is its destination's to say (`Destination::current_kind`): a
route is the current **page**, an anchor the current **location** on the page, and a language or an outside
address plainly the current one. A link with nowhere to go right now (a disabled one) is the current one of
nothing. In Rust it is `StyledContainer::current(|| …)` after `to`. `current:` without `to:` on the same box
is a build error, and a `span` takes no `current:`: a link the reader is shown as current belongs on a box.

The place under a bar comes from `use_anchor_at(line)`, which reads the anchor spanning a line of the primary
scroll's view (see [docs/primary-scroll.md](primary-scroll.md#the-place-under-a-line)); a link to a route
compares its route with the location being shown.

| Target | A current link is |
| --- | --- |
| Web, document | `aria-current="page"` for a route, `"location"` for an anchor, `"true"` for a language or an outside address, on its `<a>`; removed when it stops being current. |
| Desktop | AccessKit's `aria_current` (`Page`, `Location`, `True`) on its `Link` node. The Windows adapter hands it to UI Automation; the AT-SPI (Linux) and macOS adapters of this AccessKit release do not pass it on yet, so a reader there announces the link without it. |
| Terminal | `, current` after its role in the plain-text reading (`Web, link, current`). |
| Web canvas, headless | Carried in the snapshot for tests; there is no reader to hand it to. |
| Android | Nothing yet: no accessibility bridge (as for every role and state). |

## Links inside a paragraph

```rsx
text font_size:14
    span "Read "
    span "the repository" to:external("https://github.com/AdrielGBM/telar") color:$theme.accent
    span " or go "
    span "to the credits" to:anchor("creditos")
```

A `text` whose children are `span`s is one paragraph written as runs: shaped, wrapped and measured as one
text, each run restyled by the text properties it names (`color`, `font_weight`, `font_variation`…) and made a
link by `to:`, which takes the same destinations a box does. The text's own quoted content, if any, is the first
run. A `span` belongs inside a `text`, and a `text` with children holds `span`s and nothing else; either mistake
is a build error. In Rust it is `Text::runs(vec![TextRun::new(...).declaring(...).to(...)], ...)`, or
`Span::linking_to` on the spans of `Text::spanned`.

| Target | A link run is |
| --- | --- |
| Web, document | An `<a href>` inside the paragraph's element, the other runs `<span>`s with only what they declare. The browser answers it like any link: a plain click on this page's origin is taken back and reported as `Event::RunActivated { box_id, run }`, which the app follows. |
| Web canvas, desktop, Android, headless | Followed on a tap on its glyphs, found with `TextMetrics::index_at`; the mouse pointer takes the link shape over it. Ctrl, Cmd or Shift ask for a view beside this one, as on a box. |
| Terminal | Its style on its cells, and an OSC 8 hyperlink over them when it links outside the app. |
| Accessibility (AccessKit) | A `Link` node after its paragraph, named by its words and carrying its URL. |

## Per target

| Target | Route | Anchor | External | Modified press |
| --- | --- | --- | --- | --- |
| Web, document | `<a href>` with the serialized location; a plain same-origin click is taken back from the browser and pushed | `<a href="/current#name">`, taken back and revealed | `<a href target="_blank" rel="noopener">` for a web page; other schemes (`mailto:`) open in place, still `rel="noopener"`; the browser opens them | Left to the browser: Ctrl/Cmd/Shift and middle click open a tab or window |
| Web, canvas | `push_location` | Revealed | `window.open(uri, "_blank", "noopener")` for a web page, in place otherwise | Ctrl/Cmd/Shift open the route in a new tab |
| Desktop | `push_location` | Revealed | The `OpenURI` portal on Linux (with `system-theme`), falling back to `xdg-open`; `ShellExecuteW` on Windows; `NSWorkspace` on macOS | Followed in place |
| Android | `push_location` | Revealed | An `ACTION_VIEW` intent | — |
| Terminal | `push_location` (saved like any location) | Revealed | Enter opens it with the system launcher; its glyphs are an OSC 8 hyperlink the terminal can open itself | Followed in place |
| Headless | `push_location` | Revealed | Whatever opener a test installed with `set_uri_opener`; none by default | Followed in place |

Assistive technology activates a link the way it activates any control: on the desktop, AccessKit's `Click`
focuses it and sends Enter, and the node carries `Role::Link` and its URL. On a document the `<a>` is what a
reader activates, and the browser reports it like a click.

### The document target

The `<a>` is the link. The browser activates it on a click, on Enter and for a screen reader, and opens it
beside the page on a modified or middle click, which no tab the app opened itself could match. Telar's own
press and Enter handling stand aside there (`ui_tree::element_capture()`), and a click listener on the host takes
back exactly one case: a plain primary click on an `<a>` with no `target` or `download`, whose address is on the
page's origin. Its default is prevented and `Event::BoxActivated` is reported, so the app follows the
destination itself instead of the browser reloading the page.

The platform used to capture the pointer on every press, so a drag that leaves the host keeps arriving. A
captured pointer's `click` goes to the host rather than to what was pressed, which kept every native `<a>` from
activating. The capture now engages only once a press travels past the tap slop (`platform_core::TAP_SLOP`, 10
logical pixels), which is also the distance past which a press stops being a tap for Telar.

## Limits

- An `href` does not carry the `?telar-*` page settings a push carries, so a link opened in a new tab starts
  without them.
- A route opened beside the app exists only on the web; elsewhere a modified press follows it in place.
- Windows and macOS openers are compiled only on those systems and have not been exercised on a device.
- A link run is reached by the pointer on every target and by Tab only in a document, whose `<a>` the browser
  puts in its own order. Elsewhere a reader announces it, but it is not a focus stop: a link the keyboard must
  reach on every target belongs on a box with `to:`.
- A synthetic pointer event is untrusted, so the browser test checks that a press no longer captures the
  pointer and that a click is taken back, not that a real drag is captured past the slop.
