//! What a site says about itself beyond its pages: where it is served, the tags each page carries for search engines and link previews, and the sitemap that lists them.
//!
//! All of it is derived from `[telar.web]`, the app's catalogs and the pages the build wrote, so nothing in `.rsx` has to know a browser will read it.

use std::collections::BTreeMap;
use std::path::Path;

use i18n_core::MessageModel;
use telar_project::{OgImage, PageLocation, ThemeColor, WebSection, is_absolute_url};

use super::page::{HeadTag, escape};

/// The file the sitemap is written to, at the output root.
pub(crate) const SITEMAP_FILE: &str = "sitemap.xml";

/// The pages a build wrote, and the locales their addresses carry.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SitePages {
    /// Every page with content. With locales, each locale's root (`/es/`) stands where `/` would.
    pub(crate) pages: Vec<PageLocation>,
    /// The locales the addresses carry; empty for an app whose addresses carry none.
    pub(crate) locales: Vec<String>,
    /// The locale `/` and an address naming none fall back to; `Some` exactly when `locales` has any.
    pub(crate) base_locale: Option<String>,
    /// Whether a `404.html` was written.
    pub(crate) not_found: bool,
}

impl SitePages {
    /// A build that wrote nothing ahead of time: the root page alone.
    pub(crate) fn root_only() -> Self {
        Self {
            pages: vec![PageLocation::root()],
            ..Self::default()
        }
    }
}

/// One page, as its tags describe it.
pub(crate) struct PageMeta<'a> {
    pub(crate) title: &'a str,
    pub(crate) lang: &'a str,
    /// Where the page is; `None` for a page served at any address.
    pub(crate) location: Option<&'a PageLocation>,
    pub(crate) pages: &'a SitePages,
}

/// `[telar.web]`'s description of the site, resolved against the project: the description read from the catalog, the picture checked to exist.
#[derive(Clone, Debug)]
pub(crate) struct Site {
    origin: Option<String>,
    base: String,
    description: Description,
    og_image: Option<OgImage>,
    theme_color: Option<ThemeColor>,
}

#[derive(Clone, Debug)]
enum Description {
    Fixed(String),
    Translated {
        messages: BTreeMap<String, String>,
        fallback: String,
    },
}

impl Site {
    pub(crate) fn new(
        web: &WebSection,
        package_root: &Path,
        app_name: &str,
    ) -> Result<Self, String> {
        let description = match &web.description {
            None => Description::Fixed(format!("{app_name}, a Telar application.")),
            Some(key) => translated(key, package_root)?,
        };
        if let Some(image) = &web.og_image {
            check_images(image, &web.public_dir(package_root).0)?;
        }
        Ok(Self {
            origin: web.origin().map(str::to_string),
            base: web.base_path(),
            description,
            og_image: web.og_image.clone(),
            theme_color: web.theme_color.clone(),
        })
    }

    /// The path the site is served under, with a leading and a closing `/`.
    pub(crate) fn base(&self) -> &str {
        &self.base
    }

    pub(crate) fn has_origin(&self) -> bool {
        self.origin.is_some()
    }

    /// The path a page is served at: its directory, with the closing `/` a static host redirects to.
    pub(crate) fn path_of(&self, location: &PageLocation) -> String {
        let mut path = self.base.clone();
        for segment in location.locale.iter().chain(&location.segments) {
            path.push_str(&encode_segment(segment));
            path.push('/');
        }
        path
    }

    /// The page's full URL, when the site knows its origin.
    pub(crate) fn url_of(&self, location: &PageLocation) -> Option<String> {
        let origin = self.origin.as_ref()?;
        Some(format!("{origin}{}", self.path_of(location)))
    }

    /// The page's full URL, or its path where the site does not know its origin.
    pub(crate) fn href_of(&self, location: &PageLocation) -> String {
        self.url_of(location)
            .unwrap_or_else(|| self.path_of(location))
    }

    /// The description, in `lang` or the locale it negotiates to.
    pub(crate) fn description(&self, lang: &str) -> &str {
        match &self.description {
            Description::Fixed(text) => text,
            Description::Translated { messages, fallback } => {
                let shipped: Vec<&str> = messages.keys().map(String::as_str).collect();
                let locale = i18n_core::negotiate_locale(&[lang], &shipped, "");
                messages.get(locale).unwrap_or(fallback)
            }
        }
    }

    /// The tags a page's `<head>` carries: the base, its description, the interface color, and — where the site knows its origin — its canonical and alternate-language addresses; then what a link preview reads.
    pub(crate) fn head_tags(&self, meta: &PageMeta) -> Vec<HeadTag> {
        let mut tags = Vec::new();
        if self.base != "/" {
            tags.push(HeadTag::base(self.base.clone()));
        }
        let description = self.description(meta.lang);
        tags.push(HeadTag::meta("description", description));
        tags.extend(self.theme_color_tags());
        let canonical = meta.location.and_then(|location| self.url_of(location));
        if let Some(url) = &canonical {
            tags.push(HeadTag::link("canonical", url.clone()));
        }
        let localized = meta
            .location
            .filter(|location| location.locale.is_some() && self.has_origin());
        if let Some(location) = localized {
            tags.extend(self.alternates(location, meta.pages));
        }

        tags.push(HeadTag::property("og:type", "website"));
        tags.push(HeadTag::property("og:title", meta.title));
        tags.push(HeadTag::property("og:description", description));
        if let Some(url) = canonical {
            tags.push(HeadTag::property("og:url", url));
        }
        tags.push(HeadTag::property("og:locale", og_locale(meta.lang)));
        if let Some(location) = localized {
            let own = location.locale.as_deref();
            for locale in meta
                .pages
                .locales
                .iter()
                .filter(|l| Some(l.as_str()) != own)
            {
                tags.push(HeadTag::property("og:locale:alternate", og_locale(locale)));
            }
        }
        let locale = meta
            .location
            .and_then(|location| location.locale.as_deref())
            .unwrap_or(meta.lang);
        let base_locale = meta.pages.base_locale.as_deref().unwrap_or(locale);
        let image = self
            .og_image
            .as_ref()
            .and_then(|image| image.for_locale(locale, base_locale))
            .and_then(|source| self.image_url(source));
        let card = match image {
            Some(url) => {
                tags.push(HeadTag::property("og:image", url));
                "summary_large_image"
            }
            None => "summary",
        };
        tags.push(HeadTag::meta("twitter:card", card));
        tags
    }

    /// A `<link rel="alternate">` for the page in every locale the site carries, and `x-default` for the base locale's.
    pub(crate) fn alternates(&self, location: &PageLocation, pages: &SitePages) -> Vec<HeadTag> {
        self.alternate_hrefs(location, pages)
            .into_iter()
            .map(|(hreflang, href)| HeadTag::link("alternate", href).attr("hreflang", hreflang))
            .collect()
    }

    fn alternate_hrefs(&self, location: &PageLocation, pages: &SitePages) -> Vec<(String, String)> {
        let mut hrefs: Vec<(String, String)> = pages
            .locales
            .iter()
            .map(|locale| (locale.clone(), self.href_of(&location.in_locale(locale))))
            .collect();
        if let Some(base) = &pages.base_locale {
            hrefs.push((
                "x-default".to_string(),
                self.href_of(&location.in_locale(base)),
            ));
        }
        hrefs
    }

    /// Every page the build wrote, with its alternates, as the sitemap protocol lists them; `None` where the site does not know its origin, since a sitemap holds full URLs only.
    pub(crate) fn sitemap(&self, pages: &SitePages) -> Option<String> {
        self.origin.as_ref()?;
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n",
        );
        for page in &pages.pages {
            let url = self.url_of(page)?;
            xml.push_str(&format!("  <url>\n    <loc>{}</loc>\n", escape(&url)));
            if page.locale.is_some() {
                for (hreflang, href) in self.alternate_hrefs(page, pages) {
                    xml.push_str(&format!(
                        "    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{}\"/>\n",
                        escape(&hreflang),
                        escape(&href)
                    ));
                }
            }
            xml.push_str("  </url>\n");
        }
        xml.push_str("</urlset>\n");
        Some(xml)
    }

    fn theme_color_tags(&self) -> Vec<HeadTag> {
        match &self.theme_color {
            None => Vec::new(),
            Some(ThemeColor::One(color)) => vec![HeadTag::meta("theme-color", color.clone())],
            Some(ThemeColor::Schemes(colors)) => vec![
                HeadTag::meta("theme-color", colors.light.clone())
                    .attr("media", "(prefers-color-scheme: light)"),
                HeadTag::meta("theme-color", colors.dark.clone())
                    .attr("media", "(prefers-color-scheme: dark)"),
            ],
        }
    }

    fn image_url(&self, source: &str) -> Option<String> {
        if is_absolute_url(source) {
            return Some(source.to_string());
        }
        let origin = self.origin.as_ref()?;
        Some(format!(
            "{origin}{}{}",
            self.base,
            source.trim_start_matches('/')
        ))
    }
}

/// Open Graph spells a locale with an underscore: `es`, `en_US`.
fn og_locale(tag: &str) -> String {
    tag.replace('-', "_")
}

/// The description under `key` in every locale of the project's catalog, each a plain message: a page is described before any argument could be given.
fn translated(key: &str, package_root: &Path) -> Result<Description, String> {
    let model = telar_baker::parse_catalog(package_root)?.ok_or_else(|| {
        format!("`[telar.web] description = \"{key}\"` names a catalog key, but the project has no catalog")
    })?;
    let per_locale = model.entries.get(key).ok_or_else(|| {
        format!("`[telar.web] description = \"{key}\"` is not a key of the project's catalog")
    })?;
    let mut messages = BTreeMap::new();
    for (locale, message) in per_locale {
        let MessageModel::Plain(text) = message else {
            return Err(format!(
                "`[telar.web] description = \"{key}\"` is a message with arguments or plural forms in `{locale}`; a description is plain text"
            ));
        };
        messages.insert(locale.clone(), text.clone());
    }
    let fallback = messages
        .get(&model.default_locale)
        .or_else(|| messages.values().next())
        .cloned()
        .unwrap_or_default();
    Ok(Description::Translated { messages, fallback })
}

/// Every picture the site serves itself has to be in the public directory, the only place an unhashed file of the output comes from.
fn check_images(image: &OgImage, public: &Path) -> Result<(), String> {
    let sources: Vec<&String> = match image {
        OgImage::Shared(source) => vec![source],
        OgImage::PerLocale(sources) => sources.values().collect(),
    };
    for source in sources
        .into_iter()
        .filter(|source| !is_absolute_url(source))
    {
        let file = public.join(source.trim_start_matches('/'));
        if !file.is_file() {
            return Err(format!(
                "`[telar.web] og_image` names \"{source}\", which is not a file of the public directory ({})",
                file.display()
            ));
        }
    }
    Ok(())
}

/// A path segment as a URL writes it: the characters a segment may hold as they are, every other byte percent-encoded.
fn encode_segment(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for &byte in segment.as_bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
#[path = "site_test.rs"]
mod tests;
