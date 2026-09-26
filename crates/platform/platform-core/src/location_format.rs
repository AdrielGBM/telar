//! [`LocationFormat`]: a [`Location`] written as the path, query and fragment of a URI reference.

use std::sync::RwLock;

use crate::Location;

/// How a [`Location`] is spelled as text: `/base/segment/segment?key=value#fragment`.
///
/// One spelling for every target that carries a location as text — a browser's address bar, a `--location` argument, an Android `ACTION_VIEW` URI, the history a desktop build remembers between runs — so a link copied out of one opens the same place in another.
///
/// Segments, parameters and the fragment are percent-encoded. A path cannot hold an empty segment, so one is dropped rather than written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocationFormat {
    base: String,
    trailing_slash: bool,
}

impl Default for LocationFormat {
    fn default() -> Self {
        Self::root()
    }
}

impl LocationFormat {
    /// An app that owns its whole address space: the root location is `/`.
    pub fn root() -> Self {
        Self {
            base: "/".to_string(),
            trailing_slash: false,
        }
    }

    /// An app served under `base`. `/portfolio`, `portfolio/` and `/portfolio/` all mean `/portfolio/`.
    pub fn new(base: &str) -> Self {
        let trimmed = base.trim().trim_matches('/');
        let base = if trimmed.is_empty() {
            "/".to_string()
        } else {
            format!("/{trimmed}/")
        };
        Self {
            base,
            trailing_slash: false,
        }
    }

    /// Writes every location below the root with a closing `/` (`/es/` rather than `/es`), which is how a static host that serves each location as a directory index names it: without it, every reload costs a redirect.
    pub fn with_trailing_slash(mut self, trailing_slash: bool) -> Self {
        self.trailing_slash = trailing_slash;
        self
    }

    /// The path the root location is written as, always with a leading and a closing `/`.
    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn trailing_slash(&self) -> bool {
        self.trailing_slash
    }

    /// `location` as a path reference under the base.
    pub fn format(&self, location: &Location) -> String {
        let mut out = self.base.clone();
        let segments: Vec<String> = location
            .segments()
            .iter()
            .filter(|segment| !segment.is_empty())
            .map(|segment| encode(segment, SEGMENT))
            .collect();
        out.push_str(&segments.join("/"));
        if self.trailing_slash && !segments.is_empty() {
            out.push('/');
        }
        if !location.params().is_empty() {
            let pairs: Vec<String> = location
                .params()
                .iter()
                .map(|(key, value)| format!("{}={}", encode(key, QUERY), encode(value, QUERY)))
                .collect();
            out.push('?');
            out.push_str(&pairs.join("&"));
        }
        if let Some(fragment) = location.fragment() {
            out.push('#');
            out.push_str(&encode(fragment, FRAGMENT));
        }
        out
    }

    /// Reads a location back out of text, or `None` when it names a path outside the base.
    ///
    /// Takes what `format` writes, and also a reference relative to the base (`es/projects`) and an absolute URI, whose scheme and authority name no location and are dropped: `https://example.com/es`, `myapp:/es` and `myapp://open/es` all read as `/es`.
    pub fn parse(&self, reference: &str) -> Option<Location> {
        let reference = without_scheme(reference.trim());
        let (rest, fragment) = split_off(reference, '#');
        let (path, query) = split_off(rest, '?');
        let path = self.below_base(path)?;
        let mut location = Location::from_segments(
            path.split('/')
                .filter(|segment| !segment.is_empty())
                .map(|segment| decode(segment, false)),
        );
        for pair in query.unwrap_or_default().split('&') {
            if pair.is_empty() {
                continue;
            }
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            location = location.with_param(decode(key, true), decode(value, true));
        }
        if let Some(fragment) = fragment.filter(|fragment| !fragment.is_empty()) {
            location = location.with_fragment(decode(fragment, false));
        }
        Some(location)
    }

    fn below_base<'a>(&self, path: &'a str) -> Option<&'a str> {
        if !path.starts_with('/') {
            return Some(path);
        }
        if let Some(rest) = path.strip_prefix(self.base.as_str()) {
            return Some(rest);
        }
        (path == self.base.trim_end_matches('/')).then_some("")
    }
}

static CURRENT: RwLock<Option<LocationFormat>> = RwLock::new(None);

/// The spelling the running app's addresses are written in, for code that has to write one without holding the platform's [`LocationSource`](crate::LocationSource) — a document renderer's `href`. [`LocationFormat::root`] until a platform installs its own.
pub fn location_format() -> LocationFormat {
    CURRENT.read().unwrap().clone().unwrap_or_default()
}

/// Installs the spelling [`location_format`] answers with. Called by a platform whose addresses live under a base path.
pub fn set_location_format(format: LocationFormat) {
    *CURRENT.write().unwrap() = Some(format);
}

const SEGMENT: &[u8] = b"!$&'()*+,;=:@";
// `+`, `&`, `=` and `#` are what a query is split on, and a `+` reads back as a space.
const QUERY: &[u8] = b"!$'()*,;:@/?";
const FRAGMENT: &[u8] = b"!$&'()*+,;=:@/?";

fn encode(text: &str, allowed: &[u8]) -> String {
    let mut out = String::with_capacity(text.len());
    for &byte in text.as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) || allowed.contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn decode(text: &str, plus_is_space: bool) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = (bytes[i] == b'%' && i + 2 < bytes.len())
            .then(|| hex_pair(bytes[i + 1], bytes[i + 2]))
            .flatten();
        if let Some(byte) = escaped {
            out.push(byte);
            i += 3;
            continue;
        }
        match bytes[i] {
            b'+' if plus_is_space => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_pair(high: u8, low: u8) -> Option<u8> {
    let digit = |b: u8| (b as char).to_digit(16);
    Some((digit(high)? * 16 + digit(low)?) as u8)
}

fn split_off(text: &str, at: char) -> (&str, Option<&str>) {
    match text.split_once(at) {
        Some((before, after)) => (before, Some(after)),
        None => (text, None),
    }
}

fn without_scheme(reference: &str) -> &str {
    let scheme_end = reference
        .find(':')
        .filter(|&end| is_scheme(&reference[..end]));
    let Some(end) = scheme_end else {
        return reference;
    };
    let rest = &reference[end + 1..];
    match rest.strip_prefix("//") {
        Some(authority_and_path) => {
            let path_start = authority_and_path
                .find(['/', '?', '#'])
                .unwrap_or(authority_and_path.len());
            &authority_and_path[path_start..]
        }
        None => rest,
    }
}

fn is_scheme(candidate: &str) -> bool {
    let mut chars = candidate.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

#[cfg(test)]
#[path = "location_format_test.rs"]
mod tests;
