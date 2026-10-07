//! One [`Baker`] per registered [`AssetKind`], turning a `.rsx` `src:"path"` asset's raw bytes into the Rust
//! source expression the transpiler embeds for it.
//!
//! This is the machinery both build-time consumers share: `cargo-telar` bakes assets while building an app,
//! and `telar-analyzer` bakes them on its own so the IDE shows no errors for a `src:"…"` the app itself hasn't
//! built yet. Neither depends on the other, so the baking logic lives here instead of in either binary — and
//! instead of in `telar-transpiler`, which every app compiles and which would otherwise carry `usvg`/`resvg`/
//! `image` (~66 host crates) into a project that never bakes an asset.
//!
//! Adding a kind — fonts, shaders, audio — is an entry in [`ASSET_KINDS`](telar_project::ASSET_KINDS) plus
//! one [`Baker`] impl here; nothing in the transpiler, the macro, or a CLI needs to change to pick it up.
//!
//! A kind a plugin's component names by id rather than by path — `icon name:"mdi:home"` — also needs its ids
//! resolved into bytes before a [`Baker`] sees them. That resolver lives here too, because it runs inside the CLI
//! and the CLI loads no plugin code: the icons `[telar.icons]` configures resolve through `telar-icons-core`'s
//! sources, are judged against the package's licence policy, and are recorded in `.telar/icons.json` with the
//! notice [`ICONS_NOTICE_FILENAME`] beside it.
//!
//! A dependency's icons are baked into its own artifact, so the notice also lists the icons of every crate in the [`DependencyGraph`] the package is built with, a `[telar] library` through the record it ships.

#![warn(rustdoc::broken_intra_doc_links)]

use telar_project::AssetKind;

mod catalog;
mod icon_dependencies;
mod icons;
mod ids;
mod image;
mod package;
mod svg;
mod web_image;

pub use catalog::{
    CatalogReport, bake_catalog, catalog_files, locales_root, parse_catalog, to_source,
};
pub use icon_dependencies::{CrateIcons, DependencyGraph, IconDependency, read_library_icons};
pub use icons::{ICONS_RECORD_FILENAME, IconRecord, RecordedIcon, RecordedSet, read_icon_record};
pub use ids::{IdRef, collect_id_refs};
pub use package::{BakeReport, bake_package, bake_package_with, collect_asset_refs};
pub use telar_project::ICONS_NOTICE_FILENAME;
pub use web_image::{WEB_IMAGES_DIR, WebImage, WebImageFile, web_image};

/// Converts one registered [`AssetKind`]'s raw file bytes into the Rust source expression that reconstructs
/// the equivalent runtime data with no baking dependency left in the compiled app.
///
/// `bytes` is always the asset's raw file content, whatever its kind: a `Baker` for a text format (SVG) is
/// responsible for its own UTF-8 decoding, the same way the caller used to do it before this trait existed.
/// Keeping that conversion on the impl, not the caller, is what lets [`bakers`] stay a single uniform slice —
/// a caller that dispatched on kind to pick `&str` vs `&[u8]` is exactly the coupling this trait removes.
pub trait Baker {
    /// The asset kind this baker handles, i.e. the id `bake()` was chosen for.
    fn kind(&self) -> &'static AssetKind;
    /// Bakes `bytes` into a Rust expression, or an error message describing why they could not be baked.
    fn bake(&self, bytes: &[u8]) -> Result<String, String>;
}

/// Every registered baker, one per [`ASSET_KINDS`](telar_project::ASSET_KINDS) entry.
pub fn bakers() -> &'static [&'static dyn Baker] {
    &[&svg::SvgBaker, &image::ImageBaker, &svg::IconBaker]
}

/// The baker for the asset kind identified by [`AssetKind::id`], or `None` if `id` names no registered baker.
pub fn baker_for_id(id: &str) -> Option<&'static dyn Baker> {
    bakers().iter().copied().find(|b| b.kind().id == id)
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
