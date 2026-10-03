//! `cargo telar build --target web --prerender`: every page of the site written ahead of time, by the app itself.
//!
//! The app is built once more for the machine running the build, with `telar/prerender`, and started once per page with the request in [`PRERENDER_ENV`]. What it writes back is filled into the same template the build already expanded, so a prerendered page differs from the plain one only in what the host element holds, the state it carries and the page's own language and title.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use telar_project::{
    PRERENDER_ENV, PageLocation, PageRequest, PrerenderRequest, PrerenderSection, PrerenderedPage,
};

use super::assets;
use super::page::Page;

/// The page a static host serves for an address it has no file for.
const NOT_FOUND_FILE: &str = "404.html";

/// Builds the app's own binary for this machine, with the frontend `frontend` names and `telar/prerender`, and returns where cargo put it.
pub(super) fn build_host_binary(
    cargo_args: &[String],
    frontend: &[String],
    manifest_path: &Path,
) -> Result<PathBuf, String> {
    let mut args = vec![
        "build".to_string(),
        "--bins".to_string(),
        "--message-format=json-render-diagnostics".to_string(),
    ];
    args.extend(cargo_args.iter().filter(|a| *a != "--release").cloned());
    args.extend(frontend.iter().cloned());
    args.push("--features".to_string());
    args.push("telar/prerender".to_string());

    eprintln!("[cargo-telar] Building the app for this machine, to prerender its pages...");
    let output = Command::new("cargo")
        .args(&args)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|e| format!("failed to invoke cargo: {e}"))?;
    if !output.status.success() {
        return Err("the prerender build failed".to_string());
    }
    executable_in(&String::from_utf8_lossy(&output.stdout), manifest_path).ok_or_else(|| {
        format!(
            "the prerender build produced no binary for {}. Does this package have a `[[bin]]` whose `main` calls `run()`?",
            manifest_path.display()
        )
    })
}

/// The binary cargo reports building for the package at `manifest_path`, from its JSON messages: the one named after the package when it has several.
fn executable_in(messages: &str, manifest_path: &Path) -> Option<PathBuf> {
    let mut found = Vec::new();
    for line in messages.lines() {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if message["reason"] != "compiler-artifact"
            || !same_file(
                Path::new(message["manifest_path"].as_str().unwrap_or_default()),
                manifest_path,
            )
        {
            continue;
        }
        let Some(executable) = message["executable"].as_str() else {
            continue;
        };
        let name = message["target"]["name"].as_str().unwrap_or_default();
        found.push((name.to_string(), PathBuf::from(executable)));
    }
    let package = package_name(manifest_path);
    found
        .iter()
        .find(|(name, _)| Some(name) == package.as_ref())
        .or_else(|| found.first())
        .map(|(_, path)| path.clone())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn package_name(manifest: &Path) -> Option<String> {
    let text = std::fs::read_to_string(manifest).ok()?;
    let parsed: toml::Value = toml::from_str(&text).ok()?;
    parsed
        .get("package")?
        .get("name")?
        .as_str()
        .map(str::to_string)
}

/// Every page to write once the app has said which it has: each of its pages in each locale its address carries, or each page once when it carries none.
fn plan(discovered: &PrerenderedPage) -> Vec<PageLocation> {
    let mut pages: BTreeSet<PageLocation> = discovered
        .pages
        .iter()
        .map(|page| PageLocation {
            segments: page.segments.clone(),
            locale: None,
        })
        .collect();
    pages.insert(PageLocation::root());
    if discovered.locales.is_empty() {
        return pages.into_iter().collect();
    }
    discovered
        .locales
        .iter()
        .flat_map(|locale| pages.iter().map(move |page| page.in_locale(locale)))
        .collect()
}

/// Writes every page of the site into `out`, over the plain `index.html` the build already wrote, and a `404.html`. Returns how many pages it wrote.
pub(super) fn write_pages(
    out: &Path,
    base: &Page,
    template: &str,
    binary: &Path,
    package_root: &Path,
    settings: &PrerenderSection,
) -> Result<usize, String> {
    let scratch = std::env::temp_dir().join(format!("telar-prerender-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)
        .map_err(|e| format!("could not create {}: {e}", scratch.display()))?;
    let run = |page: PageRequest, index: usize| -> Result<PrerenderedPage, String> {
        let request = PrerenderRequest {
            out: scratch.join(format!("page-{index}.json")),
            page,
            surface: settings.surface(),
            preferences: settings.preferences(),
        };
        prerender(binary, package_root, &request)
    };

    let root = run(
        PageRequest::At {
            location: PageLocation::root(),
        },
        0,
    )?;
    write_page(
        &out.join("index.html"),
        base,
        template,
        &root,
        Some(0),
        true,
    )?;
    let mut written = 1;
    for (index, location) in plan(&root).into_iter().enumerate() {
        if location == PageLocation::root() {
            continue;
        }
        let Some(file) = location.file() else {
            eprintln!(
                "[cargo-telar] skipping the page at /{}: a segment no file path can hold",
                location.segments.join("/")
            );
            continue;
        };
        let page = run(
            PageRequest::At {
                location: location.clone(),
            },
            index + 1,
        )?;
        write_page(
            &out.join(file),
            base,
            template,
            &page,
            Some(location.depth()),
            false,
        )?;
        written += 1;
    }
    let not_found = run(PageRequest::NotFound, usize::MAX)?;
    write_page(
        &out.join(NOT_FOUND_FILE),
        base,
        template,
        &not_found,
        None,
        false,
    )?;
    let _ = std::fs::remove_dir_all(&scratch);
    Ok(written)
}

/// Runs the app once for `request` and reads back the page it wrote.
fn prerender(
    binary: &Path,
    package_root: &Path,
    request: &PrerenderRequest,
) -> Result<PrerenderedPage, String> {
    let encoded = serde_json::to_string(request).map_err(|e| e.to_string())?;
    let status = Command::new(binary)
        .env(PRERENDER_ENV, encoded)
        .current_dir(package_root)
        .status()
        .map_err(|e| format!("could not start {}: {e}", binary.display()))?;
    if !status.success() {
        return Err(format!(
            "the app exited with {status} while prerendering {}",
            describe(&request.page)
        ));
    }
    let raw = std::fs::read(&request.out)
        .map_err(|e| format!("could not read {}: {e}", request.out.display()))?;
    let page: PrerenderedPage = serde_json::from_slice(&raw).map_err(|e| {
        format!(
            "the page the app wrote for {} could not be read: {e}",
            describe(&request.page)
        )
    })?;
    if !page.settled {
        eprintln!(
            "[cargo-telar] {} was still changing when it was written; it holds the frame it had reached",
            describe(&request.page)
        );
    }
    Ok(page)
}

fn describe(page: &PageRequest) -> String {
    match page {
        PageRequest::At { location } => {
            let locale = location
                .locale
                .as_deref()
                .map(|locale| format!("/{locale}"))
                .unwrap_or_default();
            format!("the page at {locale}/{}", location.segments.join("/"))
        }
        PageRequest::NotFound => "the page for a missing address".to_string(),
    }
}

/// The plain page with what the app wrote for one place filled in. `depth` is how far below the output root the file is, which its URLs are written against; `None` is a page served at any depth, which writes them from the site's root.
fn page_for(base: &Page, page: &PrerenderedPage, depth: Option<usize>) -> Page {
    let mut filled = base.clone();
    if let Some(lang) = &page.lang {
        filled.lang = lang.clone();
        filled.dir = layout_core::Direction::for_locale(lang);
    }
    if !page.title.is_empty() {
        filled.title = page.title.clone();
    }
    filled.base = match depth {
        Some(0) => "./".to_string(),
        Some(depth) => "../".repeat(depth),
        None => "/".to_string(),
    };
    filled.host = page.host_attributes.clone();
    filled.head.push(page.head.clone());
    filled.prerendered = page.markup.clone();
    filled.state = serde_json::to_value(&page.state).ok();
    filled
}

fn write_page(
    path: &Path,
    base: &Page,
    template: &str,
    page: &PrerenderedPage,
    depth: Option<usize>,
    replace: bool,
) -> Result<(), String> {
    let html = page_for(base, page, depth)
        .render(template)
        .map_err(|e| format!("the page template: {e}"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    if replace {
        return std::fs::write(path, html)
            .map_err(|e| format!("could not write {}: {e}", path.display()));
    }
    assets::write_new(path, html.as_bytes())
}

#[cfg(test)]
#[path = "prerender_test.rs"]
mod tests;
