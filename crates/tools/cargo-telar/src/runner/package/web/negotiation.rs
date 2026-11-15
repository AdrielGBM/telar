//! The page at `/` of a site whose addresses carry a locale: not content, but a choice of which locale's root to send the reader to, made in the browser before any module loads.
//!
//! The choice is `negotiate_locale`'s, written once in JavaScript ([`NEGOTIATE_JS`]) and shared with any host profile that makes the same choice on the server, so the page, the server and the app all pick the same locale for the same preferences.

use std::collections::BTreeMap;

use i18n_core::LocaleTag;
use renderer_dom::escape_html;
use telar_project::PageLocation;

use super::page::{HeadTag, script_json};
use super::site::{PageMeta, Site, SitePages};

/// `negotiate_locale(preferred, available)`, rule for rule, with `fallback` where it finds none: for each preferred tag in order, an available tag equal to it, else the first available tag with the same primary language subtag. The available tags are canonical, and `folded` folds a preferred one as far as comparing with them needs: ASCII case, and `_` read as `-`.
pub(crate) const NEGOTIATE_JS: &str = r#"function negotiateLocale(preferred, available, fallback) {
  const folded = (tag) => String(tag).replace(/_/g, "-").replace(/[A-Z]/g, (c) => c.toLowerCase());
  const language = (tag) => folded(tag).split("-")[0];
  for (const wanted of preferred) {
    const exact = available.find((tag) => folded(tag) === folded(wanted));
    if (exact !== undefined) return exact;
    const primary = language(wanted);
    if (primary === "") continue;
    const same = available.find((tag) => language(tag) === primary);
    if (same !== undefined) return same;
  }
  return fallback;
}"#;

/// The root page: in the browser, `location.replace` to the root of the locale `navigator.languages` negotiates to, keeping the query and the fragment; without scripts, a refresh to the base locale's root and a link to every locale's. `base_locale` is the locale a reader nobody can ask is sent to, and `titles` each locale root's title, by locale.
pub(crate) fn root_page(
    site: &Site,
    pages: &SitePages,
    base_locale: &LocaleTag,
    titles: &BTreeMap<LocaleTag, String>,
) -> String {
    let base_root = PageLocation::root().in_locale(base_locale);
    let base_tag = base_locale.to_string();
    let title = titles
        .get(base_locale)
        .map(String::as_str)
        .unwrap_or(&base_tag);

    let mut head = site.head_tags(&PageMeta {
        title,
        lang: base_locale,
        location: Some(&base_root),
        pages,
    });
    if !site.has_origin() {
        head.extend(site.alternates(&base_root, pages));
    }
    head.push(HeadTag::meta("color-scheme", "light dark"));
    let head: Vec<String> = head
        .iter()
        .map(|tag| format!("    {}", tag.html()))
        .collect();

    let fallback = site.path_of(&base_root);
    let links: Vec<String> = pages
        .locales
        .iter()
        .map(|locale| {
            let tag = locale.to_string();
            let label = titles.get(locale).map(String::as_str).unwrap_or(&tag);
            format!(
                "        <li><a href=\"{}\" hreflang=\"{locale}\" lang=\"{locale}\">{}</a></li>",
                escape_html(&site.path_of(&PageLocation::root().in_locale(locale))),
                escape_html(label),
                locale = escape_html(&tag),
            )
        })
        .collect();
    let available = serde_json::Value::from(
        pages
            .locales
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
    );
    let dir = if i18n_core::Direction::for_locale(base_locale).is_rtl() {
        "rtl"
    } else {
        "ltr"
    };

    format!(
        r#"<!doctype html>
<html lang="{lang}" dir="{dir}">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{title}</title>
{head}
    <noscript><meta http-equiv="refresh" content="0; url={fallback}" /></noscript>
    <script>
{negotiate}
const preferred = navigator.languages && navigator.languages.length ? navigator.languages : [navigator.language || ""];
const locale = negotiateLocale(preferred, {available}, {base_locale});
location.replace({base} + locale + "/" + location.search + location.hash);
    </script>
  </head>
  <body>
    <noscript>
      <ul>
{links}
      </ul>
    </noscript>
  </body>
</html>
"#,
        lang = escape_html(&base_tag),
        title = escape_html(title),
        head = head.join("\n"),
        fallback = escape_html(&fallback),
        negotiate = NEGOTIATE_JS,
        available = script_json(&available),
        base_locale = script_json(&serde_json::Value::from(base_tag.as_str())),
        base = script_json(&serde_json::Value::from(site.base())),
        links = links.join("\n"),
    )
}

#[cfg(test)]
#[path = "negotiation.test.rs"]
mod tests;
