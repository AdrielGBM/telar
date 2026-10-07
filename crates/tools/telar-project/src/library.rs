//! What a `[telar] library` has to ship: every file a build that compiles it as a dependency reads, and the `Cargo.toml` `include` entries that put them in its package.
//!
//! A dependency is compiled from its Plain artifact alone, read-only, so the package has to carry that artifact beside the sources it answers for. Cargo leaves every dot-directory out of a package unless `include` names it, and `.telar/` is one, so a library that relies on cargo's defaults publishes a crate no build can compile.

use std::path::Path;

use crate::{
    ASSETS_INDEX_FILENAME, ASSETS_SOURCE_FILENAME, BuildFlavour, CATALOG_INDEX_FILENAME,
    CATALOG_SOURCE_FILENAME, MANIFEST_FILENAME, SITE_DIR,
};

const PACKAGE_TELAR_DIR: &str = ".telar";

/// The `include` entries a library's `Cargo.toml` needs, each rooted at the package: its `telar.toml`, the whole of `src/` (the `.rsx`, the Rust modules and each placement site's `.telar/`) but the module trees of the flavours a dependency is never compiled in, and the Plain flavour's artifact with the baked catalog and assets beside it.
///
/// An entry for a file the library does not produce is harmless: cargo includes what matches and nothing else.
pub fn library_include() -> Vec<String> {
    let plain = BuildFlavour::Plain;
    let other_sites = BuildFlavour::ALL
        .into_iter()
        .filter(|flavour| *flavour != plain)
        .map(|flavour| format!("!/src/**/{SITE_DIR}/{}", flavour.site_file_name()));
    [format!("/{MANIFEST_FILENAME}"), "/src/**".to_string()]
        .into_iter()
        .chain(other_sites)
        .chain([
            format!("/{PACKAGE_TELAR_DIR}/{}/**", plain.dir_name()),
            format!("/{PACKAGE_TELAR_DIR}/{}", plain.index_filename()),
            format!("/{PACKAGE_TELAR_DIR}/{CATALOG_SOURCE_FILENAME}"),
            format!("/{PACKAGE_TELAR_DIR}/{CATALOG_INDEX_FILENAME}"),
            format!("/{PACKAGE_TELAR_DIR}/{ASSETS_SOURCE_FILENAME}"),
            format!("/{PACKAGE_TELAR_DIR}/{ASSETS_INDEX_FILENAME}"),
        ])
        .collect()
}

/// Whether the [`library_include`] entry `entry` puts `relative`, a `/`-separated path from the package root, in the package. A negated entry puts nothing in.
pub fn library_include_covers(entry: &str, relative: &str) -> bool {
    if entry.starts_with('!') {
        return false;
    }
    let entry = entry.trim_start_matches('/');
    match entry.strip_suffix("/**") {
        Some(dir) => relative
            .strip_prefix(dir)
            .is_some_and(|rest| rest.starts_with('/')),
        None => relative == entry,
    }
}

/// Every file under `package_dir` that a build compiling it as a dependency reads, `/`-separated and relative to it, sorted.
///
/// That is its `telar.toml`, which is what makes it a library at all; every `.rsx` the artifact's hashes answer for and every `.rs` its module tree declares; each placement site's Plain module tree; the Plain flavour's generated directory and index, source maps included so go-to-definition lands in the `.rsx`; and the baked catalog and assets. Locale files and asset sources are not on the list: the baked artifacts carry their content, and the build only checks one against the other when it has both.
pub fn library_files(package_dir: &Path) -> Vec<String> {
    let src_dir = package_dir.join("src");
    let telar_dir = package_dir.join(PACKAGE_TELAR_DIR);
    let plain = BuildFlavour::Plain;
    let sources =
        crate::collect_files_by_ext(&src_dir, &["rs", "rsx"], &|name| !name.starts_with('.'));
    let sites = crate::placement_sites(&src_dir)
        .into_iter()
        .map(|site| site.join(SITE_DIR).join(plain.site_file_name()));
    let generated = crate::collect_files_by_ext(
        &crate::generated_dir(package_dir, plain),
        &["rs", "map"],
        &|_| true,
    );
    let artifacts = [
        plain.index_filename(),
        CATALOG_SOURCE_FILENAME,
        CATALOG_INDEX_FILENAME,
        ASSETS_SOURCE_FILENAME,
        ASSETS_INDEX_FILENAME,
    ]
    .into_iter()
    .map(|name| telar_dir.join(name));
    let mut files: Vec<String> = std::iter::once(package_dir.join(MANIFEST_FILENAME))
        .chain(sources)
        .chain(sites)
        .chain(generated)
        .chain(artifacts)
        .filter(|path| path.is_file())
        .filter_map(|path| relative_slashed(&path, package_dir))
        .collect();
    files.sort();
    files.dedup();
    files
}

fn relative_slashed(path: &Path, base: &Path) -> Option<String> {
    let relative = path.strip_prefix(base).ok()?;
    Some(
        relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

#[cfg(test)]
#[path = "library_test.rs"]
mod tests;
