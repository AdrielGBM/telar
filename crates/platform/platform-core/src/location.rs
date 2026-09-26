//! [`Location`]: a platform-neutral address for a piece of navigation state — not a URL.

/// Where a route points, in terms every Telar target understands: a path of segments, an optional fragment
/// naming an in-page anchor (see `anchor:` in `ui-core`), and query-style parameters.
///
/// This is deliberately not a URL. There is no scheme, host, or percent-encoding here — a `Location` is a
/// value a desktop deep link, an Android intent, a TUI argument, or a web `history.pushState` path can each
/// carry in their own way. Spelling one as text is [`LocationFormat`](crate::LocationFormat)'s job, never
/// this type's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Location {
    segments: Vec<String>,
    fragment: Option<String>,
    params: Vec<(String, String)>,
}

impl Location {
    /// The location with no segments, fragment, or params — the app's root.
    pub fn root() -> Self {
        Self::default()
    }

    /// Builds a location from a path's segments, in order, with no fragment or params.
    pub fn from_segments<I, S>(segments: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            segments: segments.into_iter().map(Into::into).collect(),
            fragment: None,
            params: Vec::new(),
        }
    }

    /// Appends a path segment, builder-style.
    pub fn segment(mut self, segment: impl Into<String>) -> Self {
        self.segments.push(segment.into());
        self
    }

    /// Names the in-page anchor this location targets, builder-style. Replaces any fragment already set.
    pub fn with_fragment(mut self, fragment: impl Into<String>) -> Self {
        self.fragment = Some(fragment.into());
        self
    }

    /// Appends a query-style parameter, builder-style. Repeated keys are kept in insertion order rather than
    /// overwritten, so a multi-value parameter round-trips.
    pub fn with_param(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.push((key.into(), value.into()));
        self
    }

    /// The path segments, root-to-leaf.
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// The in-page anchor this location targets, if any.
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    /// All query-style parameters, in insertion order.
    pub fn params(&self) -> &[(String, String)] {
        &self.params
    }

    /// The first value for `key`, if a parameter by that name was set.
    pub fn param(&self, key: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Whether this is the root location: no segments, fragment, or params.
    pub fn is_root(&self) -> bool {
        self.segments.is_empty() && self.fragment.is_none() && self.params.is_empty()
    }
}

#[cfg(test)]
#[path = "location_test.rs"]
mod tests;
