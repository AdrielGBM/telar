# Location

Every target has a way to point at a place inside an app: a browser's address bar, an Android link, a
command-line argument, a history a desktop app remembers between runs. Telar represents that place as one
value, `Location`, and one per-target adapter, `LocationSource`, carries it to and from the platform.

```rust
use telar::{Location, Navigator, Route};

#[derive(Clone)]
enum Page { Home, Project(String) }

impl Route for Page {
    fn to_location(&self) -> Location {
        match self {
            Page::Home => Location::root(),
            Page::Project(slug) => Location::root().segment("projects").segment(slug.clone()),
        }
    }

    fn from_location(location: &Location) -> Option<Self> {
        match location.segments() {
            [] => Some(Page::Home),
            [projects, slug] if projects == "projects" => Some(Page::Project(slug.clone())),
            _ => None,
        }
    }
}

let pages = Navigator::new(Page::Home).follow_location();
```

`follow_location` does three things:

- the stack opens on the location the app was launched or linked at;
- each push, pop, replace and reset reaches the platform as one history step;
- when the platform moves on its own (back and forward in a browser, a hash edited by hand), the stack
  follows it, and the change is not sent back to the platform.

If the platform names a location the route type does not recognize, that entry is dropped. The platform's
current entry is then **rewritten** to what the stack shows, never stepped back, so the entries the user came
from are kept. Only one navigator follows the location at a time, and the binding ends with the reactive
owner it was made under.

## The pieces

| Item | What it is |
| --- | --- |
| `Location` | Segments, an optional fragment (an in-page anchor) and query-style params. Not a URL. |
| `LocationFormat` | How a `Location` is written as text: `/base/a/b?k=v#frag`, percent-encoded, with an optional base path and trailing slash. `parse` also accepts relative references and absolute URIs, dropping their scheme and authority. |
| `location_format()` | The format the running app's addresses use, for code that writes one without holding the source (a document renderer's `href`). The web platform installs its own. |
| `LocationSource` | The platform side: `initial()`, `push`, `replace`, `back`. Each call receives the whole history, root-first. |
| `Event::LocationChanged { history }` | The platform moved on its own. The runner consumes it; the tree never sees it. |
| `WindowCommand::Navigate(HistoryUpdate)` | A move the app made, queued from widget code and applied by the runner to the source. |
| `EventHandler::on_back` | The user pressed back (Android back, a mouse's back button, a `BrowserBack` key). `false` hands the gesture back to the platform. |
| `push_location`, `replace_location`, `history_back`, `location_history` | App-side entry points that go through the following navigator, or move the history directly when none follows it. |
| `navigate_back()` | Back as the user means it: close the top dialog or drawer, otherwise step the history back. |

## Per target

| Target | Opens on | App moves | Platform moves | Back |
| --- | --- | --- | --- | --- |
| Web (DOM and canvas) | The page's address, plus the stack stored in `history.state` when the entry has one | `pushState` / `replaceState`; a pop is `history.go(-n)`, so forward still works | `popstate` and `hashchange`, reported once | The browser's own back is a `popstate` |
| Desktop | `--location <ref>` or `--location=<ref>`; without it, the history saved in `UserPrefs` | Saved to `UserPrefs` | None | Mouse back button and `BrowserBack` key; unhandled back does nothing |
| Android | The `ACTION_VIEW` intent the activity started with (`getDataString`) | Nothing: Android keeps no page history | None (see below) | Back button or gesture; unhandled back sends the task to the background (`moveTaskToBack`) |
| Terminal | `--location`, then `UserPrefs`, as on desktop | Saved to `UserPrefs` | None | None: Escape is not back |
| Headless | `HeadlessPlatform::with_location(history)`; the app's root when nothing is declared | Recorded with `record_location_into(sink)` | `with_location_changes(...)`, one before each frame | Not simulated |

Notes on the less obvious cases:

- **Web base path.** `WebOptions::location` (a `LocationFormat`) sets the base path and the trailing slash.
  Left unset, the base comes from the page's `<base href>` directory, and is `/` when there is none.
  `?telar-*` page settings are removed from the locations the app sees, and added back to every address it
  pushes, so a setting made by a link survives navigation.
- **Web scroll.** The browser's own scroll restoration is turned off (`history.scrollRestoration =
  "manual"`), because it would run before the app has drawn the page. Each entry stores the document's
  scroll position when it is left (on push, back and `pagehide`), and that position is restored over the
  next frames when the entry comes back. In-app scroll areas keep their own offsets while `NavHost` keeps
  their pages.
- **Web popping.** A pop becomes `history.go(-n)`, which the browser completes asynchronously. A push made
  before that traversal lands is overtaken by it.
- **Desktop and terminal.** A dedicated flag is used instead of the first free argument, so an app that
  takes file paths never has one read as a location. Scanning stops at `--`. There is no URI scheme
  registration; an `https://` or `myapp:` URI is accepted as the flag's value. The saved history lives under
  `history` in the app's `prefs.toml`.
- **Android.** Declaring the intent filter (scheme, host, `autoVerify` for app links) belongs to the app's
  packaging. `NativeActivity` does not forward `onNewIntent`, so a link opened while the app is already
  running reaches it only if the system starts the activity again.
- **Multiple windows.** The location belongs to the app, not to a window. Only the surface started by the
  runner holds a `LocationSource`. Windows opened next to it (`open_window`, the multi-surface runner) move no
  history, but back pressed in any of them reaches the app's one history.
- **Hot reload.** Under `cargo telar dev`, the current history is handed to every new library before its tree
  is built again.

## For a backend author

Implement `Platform::location_source` to return your adapter; the runner asks for it once, before `run`.
Report the platform's own moves as `Event::LocationChanged` with the whole history. Call
`EventHandler::on_back` for a back gesture, and do your platform's default when it returns `false`.
`FixedLocation` and `ArgumentLocation` in `platform-core` are ready-made sources for a declared location and
a command line.
