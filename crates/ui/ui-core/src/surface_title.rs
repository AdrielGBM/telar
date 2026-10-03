//! The title a surface shows, derived rather than set: the app's own title, the title of the page the app's address stands on, and the rule that puts the two together.
//!
//! Per surface, like its size: every window has a title of its own. The derived title reaches the platform as [`WindowCommand::SetTitle`], from an effect owned by the surface's world, so it lands in that surface's command queue whichever surface is active when its inputs move.

use std::rc::Rc;

use platform_core::{WindowCommand, push_window_command};
use reactive_core::{RwSignal, effect, in_surface_world, signal};

/// What a surface's title is made of, as a [`set_title_format`] rule receives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct TitleParts<'a> {
    /// The app's own title: the window configuration's, until [`set_app_title`] changes it. Empty when nothing named one.
    pub app: &'a str,
    /// The title of the page being shown, already in the active locale. `None` on a page that names none.
    pub page: Option<&'a str>,
}

type Format = Rc<dyn Fn(&TitleParts<'_>) -> String>;

#[derive(Clone, Copy)]
struct Title {
    app: RwSignal<Option<String>>,
    page: RwSignal<Option<String>>,
    format: RwSignal<Option<Format>>,
    shown: RwSignal<String>,
    // Only ever peeked: it is what the platform was last told, compared against and never followed.
    announced: RwSignal<Option<String>>,
}

reactive_core::surface_local! {
    /// The active surface's title, created with the first call that needs it.
    slot TITLE: Option<Title> = None;
    access with_title, with_title_ref;
    context SurfaceTitleContext, SurfaceTitleGuard;
}

fn existing() -> Option<Title> {
    with_title_ref(|title| *title).filter(|title| title.shown.is_alive())
}

fn title() -> Title {
    if let Some(title) = existing() {
        return title;
    }
    // In the surface's own world, so the effect lives as long as the surface and re-enters it, and with it the surface's own command queue.
    let title = in_surface_world(|| {
        let title = Title {
            app: signal(None),
            page: signal(None),
            format: signal(None),
            shown: signal(String::new()),
            announced: signal(None),
        };
        effect(move || announce(title));
        title
    });
    with_title(|slot| *slot = Some(title));
    title
}

fn announce(title: Title) {
    let app = title.app.get();
    let page = title.page.get().filter(|page| !page.is_empty());
    if app.is_none() && page.is_none() {
        return;
    }
    let parts = TitleParts {
        app: app.as_deref().unwrap_or_default(),
        page: page.as_deref(),
    };
    let composed = match title.format.get() {
        Some(format) => format(&parts),
        None => compose_title(&parts),
    };
    if title.shown.peek() != composed {
        title.shown.set(composed.clone());
    }
    if title.announced.peek().as_ref() != Some(&composed) {
        title.announced.set(Some(composed.clone()));
        push_window_command(WindowCommand::SetTitle(composed));
    }
}

/// The rule a surface's title follows unless [`set_title_format`] names another: the page's title, a dash, and the app's — `Credits — Portfolio` — or whichever of the two there is when there is only one, and the app's alone when the page carries the app's own name.
pub fn compose_title(parts: &TitleParts<'_>) -> String {
    match parts.page {
        Some(page) if parts.app.is_empty() || page == parts.app => page.to_owned(),
        Some(page) => format!("{page} — {}", parts.app),
        None => parts.app.to_owned(),
    }
}

/// Sets the app's part of the active surface's title, the part every page's title is shown beside.
pub fn set_app_title(app: impl Into<String>) {
    let app = app.into();
    let title = title();
    if title.app.peek().as_ref() != Some(&app) {
        title.app.set(Some(app));
    }
}

/// Sets the title of the page the active surface shows, already translated. A navigator that follows the app's address keeps this in step with its current route (see `Route::title`); call it directly for a page that has no route.
pub fn set_page_title(page: Option<String>) {
    // Clearing a title nobody set must not create one: it is what a navigator's teardown does, possibly after the runtime holding the store has gone.
    let title = match page {
        Some(_) => title(),
        None => match existing() {
            Some(title) => title,
            None => return,
        },
    };
    if title.page.peek() != page {
        title.page.set(page);
    }
}

/// Replaces the rule the active surface's title is composed by, [`compose_title`] by default.
///
/// The rule runs inside the effect that derives the title, so a rule that translates — or reads any other signal — is followed: the title is derived again when what it read moves.
pub fn set_title_format(format: impl Fn(&TitleParts<'_>) -> String + 'static) {
    title().format.set(Some(Rc::new(format)));
}

/// Reactive read of the title the active surface shows: what the window's title bar, the browser tab, the task switcher or the terminal says. Empty until something names one.
pub fn use_surface_title() -> String {
    title().shown.get()
}

/// Non-reactive read of [`use_surface_title`], for an event handler or a host that asks once — a prerender writing the page's `<title>`.
pub fn surface_title() -> String {
    existing()
        .map(|title| title.shown.peek())
        .unwrap_or_default()
}

/// Tells the active surface's title what its platform opened it with: `app`, the title its window was configured with, and `showing`, what the platform shows right now. The runner calls this before it builds the surface's tree; a host or a test with no runner calls it to stand in for one.
///
/// `app` only seeds the app's part: once [`set_app_title`] has named one, a later opening — a tree built again on the same surface — keeps it.
pub fn open_surface_title(app: &str, showing: &str) {
    let title = title();
    title.announced.set(Some(showing.to_owned()));
    if title.app.peek().is_none() {
        title.app.set(Some(app.to_owned()));
    }
}

#[cfg(test)]
#[path = "surface_title_test.rs"]
mod tests;
