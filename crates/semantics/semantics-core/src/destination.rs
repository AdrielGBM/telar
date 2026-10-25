//! [`Destination`]: where activating a box takes the person — a place in the app, a place on the page, the same place in another language, or something outside the app.

use std::sync::Arc;

use crate::{Location, Uri, UriError};

/// Where a link goes, as the intent rather than as an address.
///
/// Each target turns it into its own way of going there: a web page writes an `<a href>`, a desktop hands an external one to the system, a terminal marks it with OSC 8. The box carrying one is a [`Link`](crate::Role::Link) whatever else it said.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Destination {
    /// A place in this app, opened as a new entry of its history.
    Route(Location),
    /// A named anchor on the page being shown, revealed where it is.
    Anchor(Arc<str>),
    /// The place being shown, in the locale this names: a language switch that keeps the page and the anchor.
    Locale(Arc<str>),
    /// Something outside the app, opened with whatever the system opens it with.
    External(Uri),
}

impl Destination {
    pub fn anchor(name: impl Into<Arc<str>>) -> Self {
        Self::Anchor(name.into())
    }

    pub fn locale(locale: impl Into<Arc<str>>) -> Self {
        Self::Locale(locale.into())
    }

    /// An external destination at `uri`, refused unless it is a [`Uri`] the app's policy allows.
    pub fn external(uri: &str) -> Result<Self, UriError> {
        Uri::parse(uri).map(Self::External)
    }

    /// What a link here is the current one of, once marked current: the page being shown for a route, the place on that page the reader is at for an anchor, and plainly the current one of its set for a language or an outside address.
    pub fn current_kind(&self) -> CurrentKind {
        match self {
            Self::Route(_) => CurrentKind::Page,
            Self::Anchor(_) => CurrentKind::Location,
            Self::Locale(_) | Self::External(_) => CurrentKind::Item,
        }
    }
}

/// What a link marked current is the current one of, as [`Destination::current_kind`] decides: the tokens of ARIA's `aria-current`, which AccessKit models too.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CurrentKind {
    /// The page being shown, among links to pages.
    Page,
    /// The place on the page the reader is at, among links to places on it.
    Location,
    /// The current one of a set that has no more specific word for it.
    Item,
}

impl From<Location> for Destination {
    fn from(location: Location) -> Self {
        Self::Route(location)
    }
}

impl From<Uri> for Destination {
    fn from(uri: Uri) -> Self {
        Self::External(uri)
    }
}

#[cfg(test)]
#[path = "destination_test.rs"]
mod tests;
