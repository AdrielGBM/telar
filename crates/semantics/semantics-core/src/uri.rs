//! [`Uri`]: an address outside the app that names no scheme every app is refused, and [`UriSchemes`]: the schemes an app goes to, stated where a URI is opened or published.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

pub const DEFAULT_URI_SCHEMES: [&str; 5] = ["http", "https", "mailto", "tel", "sms"];

/// The schemes no app may open, whatever it allows: each runs code or reads local files in whatever opens it. A single-letter scheme is refused too, since it is a drive path (`C:\x.exe`).
pub const DENIED_URI_SCHEMES: [&str; 4] = ["javascript", "vbscript", "data", "file"];

/// The schemes an app opens and links to: [`DEFAULT_URI_SCHEMES`] and those it adds with [`allowing`](Self::allowing), compared ignoring case.
///
/// A value rather than a registry, so a [`Uri`] is judged alike on whichever thread it is: an app states it in its configuration, the runner hands it to each surface it shows, and whatever opens a URI is handed the schemes to judge it by.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct UriSchemes {
    pub(crate) added: Arc<[Arc<str>]>,
}

impl UriSchemes {
    /// These schemes and `scheme` (`"myapp"`, without the `:`). Refused for a scheme in [`DENIED_URI_SCHEMES`], a single letter, or text that is no scheme; adding one twice is harmless.
    pub fn allowing(&self, scheme: &str) -> Result<Self, UriError> {
        if !is_scheme(scheme) {
            return Err(UriError::InvalidScheme(scheme.into()));
        }
        let scheme = scheme.to_ascii_lowercase();
        if is_denied(&scheme) {
            return Err(UriError::DeniedScheme(scheme.into()));
        }
        let mut added = self.added.to_vec();
        if let Err(at) = added.binary_search_by(|held| (**held).cmp(scheme.as_str())) {
            added.insert(at, scheme.into());
        }
        Ok(Self {
            added: added.into(),
        })
    }

    /// Whether `uri` may be opened or linked to under these schemes: refused with [`UriError::SchemeNotAllowed`] when its scheme is neither a default one nor one added.
    pub fn check(&self, uri: &Uri) -> Result<(), UriError> {
        let scheme = uri.scheme().to_ascii_lowercase();
        let added = self.added.iter().any(|allowed| **allowed == *scheme);
        if DEFAULT_URI_SCHEMES.contains(&scheme.as_str()) || added {
            Ok(())
        } else {
            Err(UriError::SchemeNotAllowed(scheme.into()))
        }
    }
}

fn is_denied(lowercase_scheme: &str) -> bool {
    lowercase_scheme.len() == 1 || DENIED_URI_SCHEMES.contains(&lowercase_scheme)
}

fn is_scheme(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn reject_unsafe_characters(text: &str) -> Result<(), UriError> {
    if let Some(found) = text.chars().find(|c| c.is_whitespace() || c.is_control()) {
        return Err(UriError::ForbiddenCharacter(found));
    }
    match text.chars().find(|c| !c.is_ascii()) {
        Some(found) => Err(UriError::NotAscii(found)),
        None => Ok(()),
    }
}

/// An absolute URI that names no scheme in [`DENIED_URI_SCHEMES`]. What no app may open is refused as one is built, on any thread; whether this app goes to its scheme is for the [`UriSchemes`] stated where it is opened or published to say.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Uri(Arc<str>);

impl Uri {
    /// `text` as a URI: a scheme (RFC 3986: a letter, then letters, digits, `+`, `-` or `.`) that is not [denied](DENIED_URI_SCHEMES), a `:` and something more, all ASCII with no whitespace or control character anywhere (surrounding whitespace is refused, not trimmed).
    pub fn parse(text: &str) -> Result<Self, UriError> {
        reject_unsafe_characters(text)?;
        let (scheme, _) = text
            .split_once(':')
            .filter(|(scheme, rest)| is_scheme(scheme) && !rest.is_empty())
            .ok_or(UriError::NotAbsolute)?;
        let scheme = scheme.to_ascii_lowercase();
        if is_denied(&scheme) {
            return Err(UriError::DeniedScheme(scheme.into()));
        }
        Ok(Self(text.into()))
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

/// An address inside the app, as its `LocationFormat` writes a route: the only thing a second view of the app may be opened at, so it can never name another scheme or host.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RouteAddress(Arc<str>);

impl RouteAddress {
    /// `text` as an app-relative address: ASCII with no whitespace or control character, no scheme before the first `/`, `?` or `#`, and not a network-path reference (`//host`, `/\host`, `\\host`).
    pub fn parse(text: &str) -> Result<Self, UriError> {
        reject_unsafe_characters(text)?;
        let head = text.split(['/', '?', '#']).next().unwrap_or_default();
        let names_scheme = head
            .split_once(':')
            .is_some_and(|(scheme, _)| is_scheme(scheme));
        let network_path = ["//", "/\\", "\\"]
            .iter()
            .any(|prefix| text.starts_with(prefix));
        if names_scheme || network_path {
            return Err(UriError::NotRouteAddress);
        }
        Ok(Self(text.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RouteAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Uri {
    type Err = UriError;

    fn from_str(text: &str) -> Result<Self, UriError> {
        Self::parse(text)
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Why text is not a [`Uri`], or why a scheme cannot be allowed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum UriError {
    /// It names no scheme followed by `:` and something more.
    NotAbsolute,
    /// It holds a whitespace or control character, which a browser drops from a scheme and a launcher or a terminal reads as something else.
    ForbiddenCharacter(char),
    /// It holds a non-ASCII character, which some (U+202E, U+200B) render as something else or as nothing: percent-encode it.
    NotAscii(char),
    /// Its scheme, lowercased, is one no app may open: see [`DENIED_URI_SCHEMES`].
    DeniedScheme(Arc<str>),
    /// Its scheme, lowercased, is neither a default one nor one the [`UriSchemes`] it was judged by added.
    SchemeNotAllowed(Arc<str>),
    /// What [`UriSchemes::allowing`] was given is not a scheme.
    InvalidScheme(Arc<str>),
    /// What [`RouteAddress::parse`] was given could lead outside the app.
    NotRouteAddress,
}

impl fmt::Display for UriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAbsolute => {
                f.write_str("not an absolute URI: it names no scheme (`https:`, `mailto:`…)")
            }
            Self::ForbiddenCharacter(found) => write!(
                f,
                "it holds {found:?}, a whitespace or control character no URI may carry"
            ),
            Self::NotAscii(found) => write!(
                f,
                "it holds {found:?}, which is not ASCII: a URI must percent-encode it"
            ),
            Self::NotRouteAddress => {
                f.write_str("not an address inside the app: it names a scheme or another host")
            }
            Self::DeniedScheme(scheme) if scheme.len() == 1 => {
                write!(f, "`{scheme}:` is a drive path, not a URI scheme")
            }
            Self::DeniedScheme(scheme) => write!(f, "`{scheme}:` is a scheme no app may open"),
            Self::SchemeNotAllowed(scheme) => write!(
                f,
                "`{scheme}:` is not a scheme this app allows: add it with `UriSchemes::allowing(\"{scheme}\")` in its `AppConfig`"
            ),
            Self::InvalidScheme(scheme) => write!(
                f,
                "`{scheme}` is not a URI scheme: a letter, then letters, digits, `+`, `-` or `.`"
            ),
        }
    }
}

impl std::error::Error for UriError {}

#[cfg(test)]
#[path = "uri.test.rs"]
mod tests;
