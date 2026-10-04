//! `host = "cloudflare-pages"`: the files Cloudflare Pages reads beside the site, written from what the build knows about it.
//!
//! - `_headers`: hashed files are cached for good, pages are revalidated, and `/` varies by `Accept-Language`.
//! - `_redirects`: an address naming no locale goes to the base locale's page.
//! - `_worker.js` with `_routes.json`: `/` answers with a redirect to the locale `Accept-Language` negotiates to, by the same rule as the page standing there. Only `/` reaches the worker; everything else is served as a file.
//!
//! A `_headers` or `_redirects` of the project's own, from the public directory, is kept, and what the build derives is written after it: Pages applies every matching header rule, and the first matching redirect, so the project's own redirects win.

use std::collections::BTreeMap;
use std::path::Path;

use telar_project::PageLocation;

use super::MANIFEST_FILE;
use super::assets::{files_under, write_new};
use super::negotiation::NEGOTIATE_JS;
use super::site::{Site, SitePages};

pub(crate) const HEADERS_FILE: &str = "_headers";
pub(crate) const REDIRECTS_FILE: &str = "_redirects";
pub(crate) const WORKER_FILE: &str = "_worker.js";
pub(crate) const ROUTES_FILE: &str = "_routes.json";

/// The largest file Pages serves.
pub(crate) const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;

const MAX_HEADER_RULES: usize = 100;
const MAX_REDIRECTS: usize = 2100;

const IMMUTABLE: &str = "public, max-age=31536000, immutable";
const REVALIDATE: &str = "no-cache";

/// Writes the files Pages reads into `out`, the finished output.
pub(crate) fn write_host_files(out: &Path, site: &Site, pages: &SitePages) -> Result<(), String> {
    let manifest: BTreeMap<String, String> = {
        let path = out.join(MANIFEST_FILE);
        let raw =
            std::fs::read(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
        serde_json::from_slice(&raw)
            .map_err(|e| format!("could not read {}: {e}", path.display()))?
    };
    let images = out.join(telar_baker::WEB_IMAGES_DIR).is_dir();
    let headers = headers(site, pages, manifest.values(), images);
    extend(
        out,
        HEADERS_FILE,
        &headers,
        MAX_HEADER_RULES,
        "header rules",
    )?;
    extend(
        out,
        REDIRECTS_FILE,
        &redirects(site, pages),
        MAX_REDIRECTS,
        "redirects",
    )?;
    if let Some(base_locale) = &pages.base_locale {
        write_new(
            &out.join(WORKER_FILE),
            worker(site, &pages.locales, base_locale).as_bytes(),
        )?;
        let routes = format!(
            "{{\"version\":1,\"include\":[{}],\"exclude\":[]}}\n",
            serde_json::Value::from(site.base())
        );
        write_new(&out.join(ROUTES_FILE), routes.as_bytes())?;
    }
    Ok(())
}

/// Every file of `out` larger than Pages serves, with its size.
pub(crate) fn oversized(out: &Path) -> Result<Vec<(String, u64)>, String> {
    let mut found = Vec::new();
    for file in files_under(out)? {
        let len = std::fs::metadata(&file)
            .map_err(|e| format!("could not read {}: {e}", file.display()))?
            .len();
        if len > MAX_FILE_BYTES {
            let relative = file.strip_prefix(out).unwrap_or(&file);
            found.push((relative.display().to_string(), len));
        }
    }
    Ok(found)
}

/// The rules, as `_headers` spells them. A page rule never matches a hashed file, since Pages joins every matching rule's values and `no-cache, …, immutable` would mean neither.
fn headers<'a>(
    site: &Site,
    pages: &SitePages,
    hashed: impl IntoIterator<Item = &'a String>,
    images: bool,
) -> String {
    let mut rules: Vec<(String, Vec<(&str, &str)>)> = Vec::new();
    let mut root = vec![("Cache-Control", REVALIDATE)];
    if pages.base_locale.is_some() {
        root.push(("Vary", "Accept-Language"));
    }
    rules.push((site.base().to_string(), root));
    if pages.not_found {
        rules.push((
            format!("{}404.html", site.base()),
            vec![("Cache-Control", REVALIDATE)],
        ));
    }
    if pages.locales.is_empty() {
        for page in pages
            .pages
            .iter()
            .filter(|page| **page != PageLocation::root())
        {
            rules.push((site.path_of(page), vec![("Cache-Control", REVALIDATE)]));
        }
    } else {
        for locale in &pages.locales {
            let root = site.path_of(&PageLocation::root().in_locale(locale));
            rules.push((format!("{root}*"), vec![("Cache-Control", REVALIDATE)]));
        }
    }
    for path in hashed {
        rules.push((
            format!("{}{path}", site.base()),
            vec![("Cache-Control", IMMUTABLE)],
        ));
    }
    if images {
        rules.push((
            format!("{}{}/*", site.base(), telar_baker::WEB_IMAGES_DIR),
            vec![("Cache-Control", IMMUTABLE)],
        ));
    }
    rules
        .into_iter()
        .map(|(path, headers)| {
            let lines: Vec<String> = headers
                .into_iter()
                .map(|(name, value)| format!("  {name}: {value}"))
                .collect();
            format!("{path}\n{}\n", lines.join("\n"))
        })
        .collect()
}

/// A redirect from every page's address without a locale to the same page in the base locale, with and without its closing `/`. `/` itself is the worker's; a site whose addresses carry no locale has nothing to redirect.
fn redirects(site: &Site, pages: &SitePages) -> String {
    let Some(base_locale) = &pages.base_locale else {
        return String::new();
    };
    let mut lines = Vec::new();
    for page in pages
        .pages
        .iter()
        .filter(|page| page.locale.as_deref() == Some(base_locale) && !page.segments.is_empty())
    {
        let unlocalized = site.path_of(&PageLocation {
            segments: page.segments.clone(),
            locale: None,
        });
        let target = site.path_of(page);
        lines.push(format!(
            "{} {target} 302",
            unlocalized.trim_end_matches('/')
        ));
        lines.push(format!("{unlocalized} {target} 302"));
    }
    lines.into_iter().map(|line| format!("{line}\n")).collect()
}

/// The worker answering `/`: a `302` to the root of the locale `Accept-Language` negotiates to, varying by that header; every other request is the files'.
fn worker(site: &Site, locales: &[String], base_locale: &str) -> String {
    let json = |value: serde_json::Value| value.to_string();
    format!(
        r#"{NEGOTIATE_JS}

function acceptedLocales(header) {{
  return header
    .split(",")
    .map((part, order) => {{
      const [tag, ...parameters] = part.trim().split(";");
      const quality = parameters
        .map((parameter) => parameter.trim().toLowerCase())
        .find((parameter) => parameter.startsWith("q="));
      return {{ tag: tag.trim(), quality: quality === undefined ? 1 : Number(quality.slice(2)), order }};
    }})
    .filter(({{ tag, quality }}) => tag !== "" && tag !== "*" && quality > 0)
    .sort((a, b) => b.quality - a.quality || a.order - b.order)
    .map(({{ tag }}) => tag);
}}

const AVAILABLE = {available};
const FALLBACK = {fallback};
const BASE = {base};

export default {{
  async fetch(request, env) {{
    const url = new URL(request.url);
    if (url.pathname !== BASE) return env.ASSETS.fetch(request);
    const locale = negotiateLocale(acceptedLocales(request.headers.get("Accept-Language") ?? ""), AVAILABLE, FALLBACK);
    return new Response(null, {{
      status: 302,
      headers: {{
        Location: BASE + locale + "/" + url.search,
        Vary: "Accept-Language",
        "Cache-Control": "{REVALIDATE}",
      }},
    }});
  }},
}};
"#,
        available = json(serde_json::Value::from(locales.to_vec())),
        fallback = json(serde_json::Value::from(base_locale)),
        base = json(serde_json::Value::from(site.base())),
    )
}

/// Writes `derived` into `out/<name>` after whatever the public directory put there, refusing a file Pages would cut short.
fn extend(out: &Path, name: &str, derived: &str, limit: usize, what: &str) -> Result<(), String> {
    let path = out.join(name);
    let own = match std::fs::read_to_string(&path) {
        Ok(own) => own,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("could not read {}: {e}", path.display())),
    };
    if own.is_empty() && derived.is_empty() {
        return Ok(());
    }
    let mut content = own;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(derived);
    let count = content
        .lines()
        .filter(|line| {
            let line = line.trim_end();
            !line.is_empty() && !line.starts_with([' ', '\t', '#'])
        })
        .count();
    if count > limit {
        return Err(format!(
            "{name} would hold {count} {what}, and Cloudflare Pages reads at most {limit}"
        ));
    }
    std::fs::write(&path, content).map_err(|e| format!("could not write {}: {e}", path.display()))
}

#[cfg(test)]
#[path = "cloudflare_test.rs"]
mod tests;
