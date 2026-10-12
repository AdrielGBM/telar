//! A site whose addresses the build knows without running the app, such as the workshop's one per preview: the plain page written again at each of them, and at `404.html` for any other, so a static host serves every address straight from a file.

use std::path::{Path, PathBuf};

use telar_project::PageLocation;

use super::page::Page;
use super::prerender::{NOT_FOUND_FILE, write_page};
use super::site::{PageMeta, Site, SitePages};

/// Writes `page` at every one of `addresses` and at `404.html` into `out`, which already holds it at the root, and says which pages the site has. An address no file path can hold is left to `404.html`, with a note.
pub(super) fn write_pages(
    out: &Path,
    page: &Page,
    template: &str,
    site: &Site,
    addresses: &[PageLocation],
) -> Result<SitePages, String> {
    let files = page_files(addresses);
    let mut pages = SitePages {
        pages: std::iter::once(PageLocation::root())
            .chain(files.iter().map(|(location, _)| location.clone()))
            .collect(),
        not_found: true,
        ..SitePages::default()
    };
    pages.page_dirs = page_dirs(&pages.pages);
    for (location, file) in &files {
        let described = described(page, site, Some(location), &pages);
        write_page(&out.join(file), &described, template, false)?;
    }
    let described = described(page, site, None, &pages);
    write_page(&out.join(NOT_FOUND_FILE), &described, template, false)?;
    Ok(pages)
}

fn page_files(addresses: &[PageLocation]) -> Vec<(PageLocation, PathBuf)> {
    let mut files: Vec<(PageLocation, PathBuf)> = Vec::new();
    for location in addresses {
        if *location == PageLocation::root() || files.iter().any(|(held, _)| held == location) {
            continue;
        }
        match location.file() {
            Some(file) => files.push((location.clone(), file)),
            None => eprintln!(
                "[cargo-telar] no file is written for /{}, a segment no file path can hold; the site's 404.html still opens it",
                location.segments.join("/")
            ),
        }
    }
    files
}

fn page_dirs(pages: &[PageLocation]) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for first in pages
        .iter()
        .filter(|page| page.segments.len() > 1)
        .filter_map(|page| page.segments.first())
    {
        if !dirs.contains(first) {
            dirs.push(first.clone());
        }
    }
    dirs
}

fn described(page: &Page, site: &Site, location: Option<&PageLocation>, pages: &SitePages) -> Page {
    let mut described = page.clone();
    described.meta = site.head_tags(&PageMeta {
        title: &page.title,
        lang: &page.lang,
        location,
        pages,
    });
    described
}

#[cfg(test)]
#[path = "addresses_test.rs"]
mod tests;
