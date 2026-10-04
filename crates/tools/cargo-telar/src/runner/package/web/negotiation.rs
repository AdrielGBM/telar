//! The page at `/` of a site whose addresses carry a locale: not content, but a choice of which locale's root to send the reader to, made in the browser before any module loads.
//!
//! The choice is `negotiate_locale`'s, written once in JavaScript ([`NEGOTIATE_JS`]) and shared with any host profile that makes the same choice on the server, so the page, the server and the app all pick the same locale for the same preferences.

use std::collections::BTreeMap;

use telar_project::PageLocation;

use super::page::{HeadTag, escape, script_json};
use super::site::{PageMeta, Site, SitePages};

/// `negotiate_locale(preferred, available, fallback)`, rule for rule: for each preferred tag in order, an available tag equal to it ignoring ASCII case, else the first available tag with the same primary language subtag; `fallback` when none matches. The `lower` helper folds ASCII only, as `eq_ignore_ascii_case` does.
pub(crate) const NEGOTIATE_JS: &str = r#"function negotiateLocale(preferred, available, fallback) {
  const lower = (tag) => String(tag).replace(/[A-Z]/g, (c) => c.toLowerCase());
  const language = (tag) => lower(tag).split("-")[0];
  for (const wanted of preferred) {
    const exact = available.find((tag) => lower(tag) === lower(wanted));
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
    base_locale: &str,
    titles: &BTreeMap<String, String>,
) -> String {
    let base_root = PageLocation::root().in_locale(base_locale);
    let title = titles
        .get(base_locale)
        .map(String::as_str)
        .unwrap_or(base_locale);

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
            let label = titles.get(locale).map(String::as_str).unwrap_or(locale);
            format!(
                "        <li><a href=\"{}\" hreflang=\"{locale}\" lang=\"{locale}\">{}</a></li>",
                escape(&site.path_of(&PageLocation::root().in_locale(locale))),
                escape(label),
                locale = escape(locale),
            )
        })
        .collect();
    let available = serde_json::Value::from(pages.locales.clone());
    let dir = if layout_core::Direction::for_locale(base_locale).is_rtl() {
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
        lang = escape(base_locale),
        title = escape(title),
        head = head.join("\n"),
        fallback = escape(&fallback),
        negotiate = NEGOTIATE_JS,
        available = script_json(&available),
        base_locale = script_json(&serde_json::Value::from(base_locale)),
        base = script_json(&serde_json::Value::from(site.base())),
        links = links.join("\n"),
    )
}

#[cfg(test)]
#[path = "negotiation_test.rs"]
mod tests;
