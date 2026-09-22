//! [`Route`]: a typed app route that knows how to become, and come back from, a [`Location`].

use crate::location::Location;

/// A typed navigation destination that serializes to and from a platform-neutral [`Location`].
///
/// Implement this on the route enum an app already pushes onto a [`Navigator`](crate::Navigator) to make
/// that history addressable outside the process: [`Navigator::location`](crate::Navigator::location) and
/// [`Navigator::locations`](crate::Navigator::locations) then read the stack as `Location`s for whichever
/// [`LocationSource`] a target adapts them to (web history, a desktop deep link, an Android intent, a TUI
/// argument, or a fixed headless location).
///
/// `Navigator<R>` does not require `Route` — a route type that never leaves the process can stay a plain
/// `Clone` enum. `Route` is what a call site opts into for a route that needs an address.
///
/// # Unknown locations
///
/// [`from_location`](Self::from_location) returns `None` for a `Location` the route type does not
/// recognize — a stale link, a hand-edited deep link, or one from a future app version. Callers (a
/// `LocationSource` adapter, a prerender step) fall back to the app's own not-found route rather than
/// panicking.
pub trait Route: Clone {
    /// Serializes this route to its neutral location.
    fn to_location(&self) -> Location;

    /// Parses a location back into this route type, or `None` if it names no known route.
    fn from_location(location: &Location) -> Option<Self>
    where
        Self: Sized;

    /// A human-readable title for this route, if it has one — the hook a surface's title (T-9.1) derives
    /// from. Routes that do not name a page (a dialog route, an in-page anchor) leave this `None`.
    fn title(&self) -> Option<String> {
        None
    }
}

#[cfg(test)]
#[path = "route_test.rs"]
mod tests;
