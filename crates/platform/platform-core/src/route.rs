//! [`Route`]: a typed app route that knows how to become, and come back from, a [`Location`].

use crate::Location;

/// A typed navigation destination that serializes to and from a platform-neutral [`Location`].
///
/// Implement this on the route enum an app already pushes onto a `Navigator` to make
/// that history addressable outside the process: `Navigator::location` and
/// `Navigator::locations` then read the stack as `Location`s for whichever
/// [`LocationSource`](crate::LocationSource) a target adapts them to (web history, a desktop deep link, an Android intent, a TUI
/// argument, or a fixed headless location).
///
/// `Navigator<R>` does not require `Route` — a route type that never leaves the process can stay a plain
/// `Clone` enum. `Route` is what a call site opts into for a route that needs an address.
///
/// # Unknown locations
///
/// [`from_location`](Self::from_location) returns `None` for a `Location` the route type does not
/// recognize — a stale link, a hand-edited deep link, or one from a future app version. `Navigator::follow_location`
/// leaves such an entry out; an app that wants a not-found page instead answers with one from here.
pub trait Route: Clone {
    /// Serializes this route to its neutral location.
    fn to_location(&self) -> Location;

    /// Parses a location back into this route type, or `None` if it names no known route.
    fn from_location(location: &Location) -> Option<Self>
    where
        Self: Sized;

    /// The title of the page this route shows, in the active locale, if it names one: what the surface's title is derived from while a navigator following the app's address stands on it (see `docs/surface-title.md`). Routes that do not name a page (a dialog route, an in-page anchor) leave this `None`.
    ///
    /// Called inside an effect, so a title translated here — `t!("credits.title")`, or `telar::t(key)` for a key the route holds — follows a change of locale without anything else asking.
    fn title(&self) -> Option<String> {
        None
    }

    /// Every page of this route type that stands on an address of its own, root first: what a tool that visits each page reads while a navigator following the app's address is built on it — a prerender writing one page per location and locale (see `docs/prerender.md`), a sitemap.
    ///
    /// Leave out a route that needs something only the running app has, such as a search result or a record fetched at run time: it is still opened by its address, just not written ahead. Empty by default, which leaves the app's root as its only page.
    fn pages() -> Vec<Self>
    where
        Self: Sized,
    {
        Vec::new()
    }
}

#[cfg(test)]
#[path = "route_test.rs"]
mod tests;
