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

A fragment names an anchor on a page, not a page (see [docs/links.md](links.md#anchors)). The navigator sees
the pages a history shows, `pages_of(history)`: every entry without its fragment, and an entry naming an anchor on
the page before it folded into that page. The history itself keeps those entries, so back from the next page
returns to the anchor the reader left, and back from an anchor entry leaves the page where it is. Pushing a
location with a fragment opens its page, if that is not the one shown, then adds the anchor entry and reveals it.

A locale names the language a page is shown in, not a page either: see [The locale in the
location](#the-locale-in-the-location).

The current route's `Route::title` also names the page in the surface's title, `Credits — Portfolio`, in the active locale; see [docs/surface-title.md](surface-title.md).

The pages a route type lists in `Route::pages` are the app's, read back with `location_pages()` while a
navigator built on it follows the address: what a prerender writes one page for, per locale (see
[docs/prerender.md](prerender.md)).

If the platform names a location the route type does not recognize, that entry is dropped. The platform's
current entry is then **rewritten** to what the stack shows, never stepped back, so the entries the user came
from are kept. Only one navigator follows the location at a time, and the binding ends with the reactive
owner it was made under.

## The pieces

| Item | What it is |
| --- | --- |
| `Location` | Segments, an optional locale, an optional fragment (an in-page anchor) and query-style params. Not a URL. |
| `LocationFormat` | How a `Location` is written as text: `/base/es/a/b?k=v#frag`, percent-encoded, with an optional base path and trailing slash. `parse` also accepts relative references and absolute URIs, dropping their scheme and authority, and reads a first segment as the locale only for the locales `with_locales` names. |
| `location_format()` | The format the running app's addresses use, for code that writes one without holding the source (a document renderer's `href`, a `--location` argument). The web platform installs its own; both read the locales the app declared. |
| `LocationSource` | The platform side: `initial()`, `push`, `replace`, `back`. Each call receives the whole history, root-first. |
| `Event::LocationChanged { history }` | The platform moved on its own. The runner consumes it; the tree never sees it. |
| `WindowCommand::Navigate(HistoryUpdate)` | A move the app made, queued from widget code and applied by the runner to the source. |
| `EventHandler::on_back` | The user pressed back (Android back, a mouse's back button, a `BrowserBack` key). `false` hands the gesture back to the platform. |
| `push_location`, `replace_location`, `history_back`, `location_history` | App-side entry points that go through the following navigator, or move the history directly when none follows it. |
| `push_anchor`, `pages_of` | Add an entry naming an anchor on the current page and reveal it; the pages a history shows. |
| `set_anchor_revealer` | What reveals an anchor the history arrives at; installed by the tree's anchor registry, which is handed the one the app opened at once it exists. |
| `navigate_back()` | Back as the user means it: close the top dialog or drawer, otherwise step the history back. |
| `follow_location_locale(available, base)` | Opts the app into carrying its locale in its address; see below. |
| `location_locale`, `location_locales` | The locale the address is written in, and the ones it can carry. |
| `bind_location_locale`, `LocaleFollower`, `set_location_locale` | The `platform-core` side of that binding, which `follow_location_locale` installs. |
| `rename_anchor` | An anchor took a new name (one that follows the locale); the history names it by the new one. `anchor:` reports it. |

## The locale in the location

An app that ships several languages can make the one it is shown in part of where the reader is, so a link,
a reload, a deep link or a restored session opens in that language:

```rust
telar::app!(MyTheme, {
    telar::follow_location_locale(["es", "en"], "es");
}, config, Root);
```

| | What happens |
| --- | --- |
| Opening | The address decides: `/en/projects` opens in English. An address that names no locale is written in the first of: the locale already set, the one the person last chose (`telar.locale`, see [docs/user-preferences.md](user-preferences.md)), and `negotiate_locale` of the system's preferred locales against `available`, falling back to `base`. The platform's entry is then rewritten to name it. |
| `set_locale(tag)` | The entry shown is rewritten in the new locale, keeping the page and the anchor; no entry is added. A regional tag is carried as the locale it negotiates to (`en-GB` as `en`), and one the app does not ship as `base`. The choice is kept for the next run. |
| A link to a `Location` with a locale | Switches to it (the same rewrite), then opens the place if it is not the one shown. A link to the current place in another language is a language switch and nothing more. |
| `to:in_locale("en")` | The language switcher: the place being shown, in that locale. Its `<a href>` is the current entry's address in it (`/en/doc#team`), and following it is the same switch as `set_locale`. |
| Back and forward | An entry the platform returns to from before a switch is shown, and rewritten, in the locale the app is in. |
| Routes | Never see a locale: `Route::to_location` writes none, `from_location` is handed none, `pages_of` strips it. |
| Links | `address_of` writes a route or an anchor in the locale the address carries, so an `<a href>` is right before any code runs. |
| An anchor named by `t!(…)` | Renamed in the address when the locale changes (`/es/#simulacion` becomes `/en/#simulation`), provided the same box re-reads its name rather than being rebuilt. |

**Why a switch replaces rather than pushes.** A language is how a place is shown, not a place: back after a
switch should leave the page, as it does after a theme change, and should not quietly undo the reader's
choice. Since the whole history is written in one locale, the entries before a switch are rewritten as the
reader returns to them, which keeps the address and what is on screen in agreement. Every entry still names
its locale, so a copied link, a reload and a prerendered page each open in the right one.

| Target | Where the locale lives |
| --- | --- |
| Web (DOM and canvas) | The first segment under the base path: `/es/`, `/portfolio/en/projects#team`. `/` is handled like any address without a locale; serving it as a negotiation page belongs to the web packaging ([docs/web-packaging.md](web-packaging.md#the-root-of-a-site-in-several-locales)). |
| Desktop, terminal | In every entry of the history saved in `UserPrefs`, and in `--location /en/projects`. Also kept as `telar.locale`, for a launch with no saved history. |
| Android | The `ACTION_VIEW` link (`https://example.com/en/projects`), and `telar.locale` for a launch without one. |
| Headless | The fixed location: `HeadlessPlatform::with_location([route.to_location().with_locale("en")])` is one route in one language, which is what a prerender of each route × locale builds. |

An app that never calls `follow_location_locale` has addresses with no locale in them, and `/es/projects` is
two segments there as it always was.

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
  Left unset, the base comes from the page's `<base href>` directory, and is `/` when there is none; a packaged
  build writes that `<base href>` from `[telar.web] base`. A format that names no locales reads the ones the
  app declared.
  `?telar-*` page settings are removed from the locations the app sees, and added back to every address it
  pushes, so a setting made by a link survives navigation.
- **Web scroll.** The browser's own scroll restoration is turned off (`history.scrollRestoration =
  "manual"`), because it would run before the app has drawn the page. Each entry stores the page's
  scroll position (the surface's primary scroll, see [docs/primary-scroll.md](primary-scroll.md)) when it is left (on push, back and `pagehide`), and that position is restored over the
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
