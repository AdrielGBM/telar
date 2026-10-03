# Surface title

Every surface has a name the platform shows for it: a browser tab, a window's title bar, the recents screen,
a terminal's tab. In Telar that name is derived rather than set. It comes from three things, in this order:

1. the **app's title**, the `title` of the `WindowConfig` the surface was opened with;
2. the **page's title**, the title of the route the app's address stands on;
3. the **locale**, because the page's title is translated, and changes with it.

```rust
use telar::{Location, Navigator, Route, t};

#[derive(Clone)]
enum Page { Home, Credits }

impl Route for Page {
    fn to_location(&self) -> Location { /* … */ }
    fn from_location(location: &Location) -> Option<Self> { /* … */ }

    fn title(&self) -> Option<String> {
        match self {
            Page::Home => None,
            Page::Credits => Some(t!("credits.title")),
        }
    }
}

let pages = Navigator::new(Page::Home).follow_location();
```

With the app titled `Portfolio`, the surface is called `Portfolio` on the home page and `Credits — Portfolio`
on the credits page; after `set_locale("es")` it is `Créditos — Portfolio`. Nothing else has to ask: the title
is derived in an effect, so it moves when the route, the locale or anything else the title reads moves.

## The pieces

| Item | What it is |
| --- | --- |
| `Route::title` | The page's title in the active locale, or `None`. Translate with `t!("key")`, or `telar::i18n::t(key, &[])` for a key the route holds. |
| `Navigator::follow_location` | Besides following the address, keeps the page's title in step with the current route. Only the navigator that follows the address names the page. |
| `set_page_title(Option<String>)` | Names the page directly, for a page that has no route. |
| `telar::window::set_title(app)` | Renames the app's part of this window's title. |
| `set_title_format(rule)` | Replaces the rule the title is composed by. The rule gets `TitleParts { app, page }`. |
| `compose_title(parts)` | The default rule: `page — app`, either alone when the other is missing, and the app alone when the page carries the app's own name. |
| `use_surface_title()` / `telar::window::title()` | Reactive read of the title the surface shows, for a custom title bar. |
| `surface_title()` | Non-reactive read, for an event handler or a host that asks once. |
| `open_surface_title(app, showing)` | What the runner tells the store before it builds the tree: the configured title and what the platform shows now. |

A rule passed to `set_title_format` runs inside the effect that derives the title, so a rule that translates
follows the locale too:

```rust
telar::set_title_format(|parts| match parts.page {
    Some(page) => format!("{page} | {}", parts.app),
    None => parts.app.to_string(),
});
```

## Per target

The derived title reaches the platform through `Window::set_title`, once each time it changes.

| Target | Shows it as | Notes |
| --- | --- | --- |
| Web (DOM and canvas) | `document.title` | The tab, the history entry, a bookmark. The page opens on the configured title. |
| Desktop | The window's title | The title bar and the task switcher, through winit. |
| Android | The task description | `Activity.setTaskDescription`, the label on the recents screen. Until the app derives a title of its own, the task keeps the label from the manifest. |
| Terminal | OSC 0 | The terminal's window title and icon name, which is its tab in most terminals. The terminal's own titles are saved on entry (`CSI 22;0 t`) and given back on exit (`CSI 23;0 t`). Until the app derives a title of its own, the terminal keeps its title. |
| Headless | `HeadlessWindow::title()` | Kept, and written into a `TitleSink` with `HeadlessPlatform::record_titles_into`. |

Notes on the less obvious cases:

- **What the window opens with.** Every platform opens on the configured title where it has a place for one
  (web, desktop, headless). Android and a terminal already show a title of their own, the app's label and
  whatever the shell set, so they are left alone until the derived title differs from the configured one.
- **More than one window.** The title belongs to the surface, not to the app. Each window opened beside the
  primary one derives its own from its own `WindowConfig`; only the surface whose navigator follows the
  address has a page title from it.
- **A tree built again.** A remount keeps a title the app set with `telar::window::set_title`. Under
  `cargo telar dev` the new library is told the configured title and what the window shows, and derives its
  own from there.

## Reading it in a prerender

A headless run derives the same title the browser would show, so a tool that writes a page per location and
locale reads it from there:

```rust
use platform_headless::{HeadlessPlatform, TitleSink};

telar::set_locale("es");
let titles = TitleSink::default();
telar::run_with_platform::<_, _, ()>(
    HeadlessPlatform::new(1280, 800)
        .with_location([location])
        .record_titles_into(titles.clone())
        .with_frames(2),
    config,
    paths,
    app,
    "prerender",
)?;
let title = titles.lock().unwrap().last().cloned();
```

The first title recorded is the configured one the window opened with; the last is the page's.

## For a backend author

Implement `Window::set_title`. Show the configured title when the window opens if your platform has a
place for one; the runner assumes the window shows it. A platform with nowhere to show a title leaves the
default no-op.
