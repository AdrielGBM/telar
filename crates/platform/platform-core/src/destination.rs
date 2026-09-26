//! The app's side of a [`Destination`]: what can name one, and the address a target writes for it.

pub use semantics_core::{Destination, Uri};

use crate::{Location, Route, location_format, location_history};

/// A value a link can be pointed at: a [`Destination`] itself, a [`Location`], or any typed [`Route`].
pub trait IntoDestination {
    fn into_destination(self) -> Destination;
}

impl IntoDestination for Destination {
    fn into_destination(self) -> Destination {
        self
    }
}

impl IntoDestination for Location {
    fn into_destination(self) -> Destination {
        Destination::Route(self)
    }
}

impl<R: Route> IntoDestination for R {
    fn into_destination(self) -> Destination {
        Destination::Route(self.to_location())
    }
}

/// A link to the anchor `name` on the page being shown.
pub fn anchor(name: &str) -> Destination {
    Destination::anchor(name)
}

/// A link to `uri`, outside the app.
///
/// # Panics
///
/// When `uri` names no scheme; see [`Destination::external`].
pub fn external(uri: &str) -> Destination {
    Destination::external(uri)
}

/// `destination` as the text a target writes where an address goes — an `href`, an accessibility URL: a route in the app's [`location_format`], an anchor as the current location with that fragment, an external URI as itself.
pub fn address_of(destination: &Destination) -> String {
    match destination {
        Destination::Route(location) => location_format().format(location),
        Destination::Anchor(name) => {
            let current = location_history().pop().unwrap_or_default();
            location_format().format(&current.with_fragment(name.to_string()))
        }
        Destination::External(uri) => uri.to_string(),
    }
}

#[cfg(test)]
#[path = "destination_test.rs"]
mod tests;
