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
name a scheme (`https:`, `mailto:`…): a literal without one is a build error on the attribute, and the Rust
function panics.

## What a link is

| Item | What it is |
| --- | --- |
| `Destination` | `Route(Location)`, `Anchor(name)` or `External(Uri)`. Lives in `semantics-core`, beside `Location`, because a renderer and a platform both read it. |
| `Semantics::link` | The destination a box's element carries. `Semantics::linking_to` sets it and the `link` role together. |
| `IntoDestination` | What `to` accepts: a `Destination`, a `Location`, or any `Route`. `Route` now lives in `platform-core` (re-exported by `navigate-core` as before) so this works without the `navigate` feature. |
| `anchor`, `external` | The two constructors `.rsx` names. |
| `address_of` | A destination as the text a target writes where an address goes: a route in the app's `location_format()`, an anchor as the current location with that fragment, an external URI as itself. |
| `follow`, `follow_beside`, `follow_pressed` | Going there from app code: push the route, reveal the anchor, open the URI. `follow_pressed` reads Ctrl, Cmd or Shift as a request for a view beside this one. |
| `open_uri`, `UriOpener`, `set_uri_opener` | The `services-core` service that hands a URI to the system. Every runner installs its platform's; headless installs none, and a test installs its own. |
| `register_anchor`, `reveal_anchor` | The lookup an anchor resolves through. `anchor:` (the attribute that names a box as an anchor) is still to come; until a box registers under a name, following an anchor to it does nothing. |
| `Event::BoxActivated` | A surface that follows links itself reporting a plain activation back to the app. |

A link is a control: it joins the tab order with the `link` role, follows on a tap and on **Enter** (not Space,
which scrolls the page a link sits on), keeps no keys from a host page, and is announced with its address. An
`on_press` on the same box still runs, before the link is followed. A disabled link goes nowhere. Only `box` is a
link host in Rust (`StyledContainer::to`); a `col`, `row` or `grid` with `to:` is built as one, since a link has
to be focusable and show a focus ring.

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

- An anchor does nothing until `anchor:` lands and registers boxes under their names.
- An `href` does not carry the `?telar-*` page settings a push carries, so a link opened in a new tab starts
  without them.
- A route opened beside the app exists only on the web; elsewhere a modified press follows it in place.
- Windows and macOS openers are compiled only on those systems and have not been exercised on a device.
- A synthetic pointer event is untrusted, so the browser test checks that a press no longer captures the
  pointer and that a click is taken back, not that a real drag is captured past the slop.
