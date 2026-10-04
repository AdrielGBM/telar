//! The keys of `[telar.web]` that describe the site rather than the build: where it is served, what it says about itself to a crawler or a link preview, and which host its output is shaped for.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::manifest::WebSection;

/// The static host a browser build is shaped for: the files it writes beside the site, and whether it precompresses.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WebHost {
    /// Any server that maps paths to files. Release builds carry `.br`/`.gz` copies for one that serves them.
    #[default]
    Static,
    /// Cloudflare Pages: `_headers`, `_redirects`, and a worker answering `/`; no precompressed copies, since the edge compresses.
    CloudflarePages,
}

/// The picture a link preview shows: one for every locale, or one per locale.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum OgImage {
    Shared(String),
    PerLocale(BTreeMap<String, String>),
}

impl OgImage {
    /// The picture for a page in `locale`; a locale the table leaves out shows `base`'s.
    pub fn for_locale(&self, locale: &str, base: &str) -> Option<&str> {
        match self {
            Self::Shared(image) => Some(image),
            Self::PerLocale(images) => images
                .get(locale)
                .or_else(|| images.get(base))
                .map(String::as_str),
        }
    }

    fn sources(&self) -> Vec<&str> {
        match self {
            Self::Shared(image) => vec![image],
            Self::PerLocale(images) => images.values().map(String::as_str).collect(),
        }
    }
}

/// The color a browser paints its own interface in before the app has drawn: one, or one per color scheme.
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum ThemeColor {
    One(String),
    Schemes(SchemeColors),
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SchemeColors {
    pub light: String,
    pub dark: String,
}

/// Whether `source` names a picture by a full URL rather than a file the site serves.
pub fn is_absolute_url(source: &str) -> bool {
    source.starts_with("https://") || source.starts_with("http://")
}

impl WebSection {
    /// The path the site is served under, with a leading and a closing `/`: `/` unless `base` names another.
    pub fn base_path(&self) -> String {
        let trimmed = self.base.as_deref().unwrap_or("/").trim().trim_matches('/');
        if trimmed.is_empty() {
            "/".to_string()
        } else {
            format!("/{trimmed}/")
        }
    }

    /// The scheme and host the site is served at, with no closing `/`.
    pub fn origin(&self) -> Option<&str> {
        self.origin
            .as_deref()
            .map(|origin| origin.trim().trim_end_matches('/'))
    }

    pub fn host(&self) -> WebHost {
        self.host.unwrap_or_default()
    }

    /// Everything wrong with the table that the schema alone cannot see, one message each.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if let Some(origin) = self.origin()
            && !is_origin(origin)
        {
            problems.push(format!(
                "`[telar.web] origin = \"{origin}\"` is not a scheme and a host, like \"https://example.com\""
            ));
        }
        if let Some(base) = &self.base
            && !is_base_path(base)
        {
            problems.push(format!(
                "`[telar.web] base = \"{base}\"` is not a path, like \"/\" or \"/docs/\""
            ));
        }
        let relative_image = self
            .og_image
            .iter()
            .flat_map(OgImage::sources)
            .find(|source| !is_absolute_url(source));
        if let Some(image) = relative_image
            && self.origin.is_none()
        {
            problems.push(format!(
                "`[telar.web] og_image` names \"{image}\", a file of the site, but a link preview needs its full URL; set `origin`"
            ));
        }
        if self.host() == WebHost::CloudflarePages && self.base_path() != "/" {
            problems.push(format!(
                "`[telar.web] host = \"cloudflare-pages\"` serves the site at the root of its domain, so `base = \"{}\"` cannot be where it is",
                self.base.as_deref().unwrap_or_default()
            ));
        }
        problems
    }
}

fn is_origin(origin: &str) -> bool {
    let Some(authority) = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))
    else {
        return false;
    };
    !authority.is_empty()
        && !authority.contains(['/', '?', '#', '@', '\\'])
        && !authority.chars().any(char::is_whitespace)
}

fn is_base_path(base: &str) -> bool {
    let trimmed = base.trim().trim_matches('/');
    trimmed.is_empty()
        || trimmed.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && !segment.contains(['?', '#', '\\', '%'])
                && !segment.chars().any(char::is_whitespace)
        })
}

#[cfg(test)]
#[path = "web_test.rs"]
mod tests;
