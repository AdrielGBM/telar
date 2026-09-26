//! [`Destination`]: where activating a box takes the person — a place in the app, a place on the page, or something outside the app.

use std::fmt;
use std::sync::Arc;

use crate::Location;

/// Where a link goes, as the intent rather than as an address.
///
/// Each target turns it into its own way of going there: a web page writes an `<a href>`, a desktop hands an external one to the system, a terminal marks it with OSC 8. The box carrying one is a [`Link`](crate::Role::Link) whatever else it said.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Destination {
    /// A place in this app, opened as a new entry of its history.
    Route(Location),
    /// A named anchor on the page being shown, revealed where it is.
    Anchor(Arc<str>),
    /// Something outside the app, opened with whatever the system opens it with.
    External(Uri),
}

impl Destination {
    pub fn anchor(name: impl Into<Arc<str>>) -> Self {
        Self::Anchor(name.into())
    }

    /// An external destination at `uri`, or `None` when `uri` names no scheme (`https:`, `mailto:`…) — a runtime
    /// string built from user data, unlike a literal in `.rsx`, which the transpiler refuses at build time.
    pub fn external(uri: &str) -> Option<Self> {
        Uri::parse(uri).map(Self::External)
    }
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

/// An absolute URI: a scheme, a `:`, and whatever that scheme says follows it. Checked for the scheme and nothing else, because what follows is the business of whoever opens it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Uri(Arc<str>);

impl Uri {
    /// `text` as a URI, or `None` when it does not start with a scheme (RFC 3986: a letter, then letters, digits, `+`, `-` or `.`) followed by `:` and something more.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (scheme, rest) = text.split_once(':')?;
        let mut chars = scheme.chars();
        let starts_with_letter = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
        let valid = starts_with_letter
            && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
            && !rest.is_empty();
        valid.then(|| Self(text.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The scheme, as written.
    pub fn scheme(&self) -> &str {
        self.0.split_once(':').map_or("", |(scheme, _)| scheme)
    }

    /// Whether this is a web page (`http` or `https`), which a browser opens beside the app rather than in place of it.
    pub fn is_web(&self) -> bool {
        let scheme = self.scheme();
        scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
#[path = "destination_test.rs"]
mod tests;
