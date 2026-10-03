//! The app's side of a [`Destination`]: what can name one, and the address a target writes for it.

pub use semantics_core::{Destination, Uri};

use crate::history::in_location_locale;
use crate::{Location, Route, location_format, location_history};

/// A value a link can be pointed at: a [`Destination`] itself, a [`Location`], or any typed [`Route`] — or,
/// flattened, an `Option` of one of those, so `to:external(expr)` and `to:(maybe_route)` both work when the
/// expression has nowhere to go right now.
pub trait IntoDestination {
    fn into_destination(self) -> Option<Destination>;
}

impl IntoDestination for Destination {
    fn into_destination(self) -> Option<Destination> {
        Some(self)
    }
}

impl IntoDestination for Location {
    fn into_destination(self) -> Option<Destination> {
        Some(Destination::Route(self))
    }
}

impl<R: Route> IntoDestination for R {
    fn into_destination(self) -> Option<Destination> {
        Some(Destination::Route(self.to_location()))
    }
}

impl<D: IntoDestination> IntoDestination for Option<D> {
    fn into_destination(self) -> Option<Destination> {
        self.and_then(IntoDestination::into_destination)
    }
}

/// A link to the anchor `name` on the page being shown.
pub fn anchor(name: &str) -> Destination {
    Destination::anchor(name)
}

/// A link to the place being shown, in `locale`: a language switch that keeps the page and the anchor. Written as that place's address in `locale`, so a page's `<a href>` opens it there before any code runs. Only an app whose address carries its locale can go there; elsewhere it is the place being shown, and following it goes nowhere.
pub fn in_locale(locale: &str) -> Destination {
    Destination::locale(locale)
}

fn shown_in(locale: &str) -> Location {
    let shown = location_history().pop().unwrap_or_default();
    let carried = crate::location_locales()
        .into_iter()
        .find(|carried| carried.eq_ignore_ascii_case(locale));
    match carried {
        Some(locale) => shown.with_locale(locale),
        None => shown,
    }
}

/// A link to `uri`, outside the app, or `None` when `uri` names no scheme; see [`Destination::external`].
///
/// A build-time literal without a scheme is a transpiler error, so `None` here means a runtime string built from
/// user data was rejected — logged once, naming the string, since the caller has nowhere else to learn why its
/// link went nowhere.
pub fn external(uri: &str) -> Option<Destination> {
    let destination = Destination::external(uri);
    if destination.is_none() {
        tracing::warn!("external(\"{uri}\") is not an absolute URI: it names no scheme");
    }
    destination
}

/// `destination` as the text a target writes where an address goes — an `href`, an accessibility URL: a route in the app's [`location_format`] and in the locale its address carries, an anchor as the current location with that fragment, a locale as the current location in it, an external URI as itself.
pub fn address_of(destination: &Destination) -> String {
    match destination {
        Destination::Route(location) => {
            location_format().format(&in_location_locale(location.clone()))
        }
        Destination::Anchor(name) => {
            let current = in_location_locale(location_history().pop().unwrap_or_default());
            location_format().format(&current.with_fragment(name.to_string()))
        }
        Destination::Locale(locale) => location_format().format(&shown_in(locale)),
        Destination::External(uri) => uri.to_string(),
    }
}

#[cfg(test)]
#[path = "destination_test.rs"]
mod tests;
