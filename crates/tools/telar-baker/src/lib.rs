//! One [`Baker`] per registered [`AssetKind`], turning a `.rsx` `src:"path"` asset's raw bytes into the Rust
//! source expression the transpiler embeds for it.
//!
//! This is the machinery both build-time consumers share: `cargo-telar` bakes assets while building an app,
//! and `telar-analyzer` bakes them on its own so the IDE shows no errors for a `src:"…"` the app itself hasn't
//! built yet. Neither depends on the other, so the baking logic lives here instead of in either binary — and
//! instead of in `telar-transpiler`, which every app compiles and which would otherwise carry `usvg`/`resvg`/
//! `image` (~66 host crates) into a project that never bakes an asset.
//!
//! Adding a kind — fonts, shaders, audio — is an entry in [`ASSET_KINDS`](telar_transpiler::ASSET_KINDS) plus
//! one [`Baker`] impl here; nothing in the transpiler, the macro, or a CLI needs to change to pick it up.

#![warn(rustdoc::broken_intra_doc_links)]

use telar_transpiler::AssetKind;

mod catalog;
mod image;
mod package;
mod svg;

pub use catalog::{
    CatalogReport, bake_catalog, catalog_files, locales_root, parse_catalog, to_source,
};
pub use package::{BakeReport, bake_package, collect_asset_refs, resolve_telar_version};

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

/// Every registered baker, one per [`ASSET_KINDS`](telar_transpiler::ASSET_KINDS) entry.
pub fn bakers() -> &'static [&'static dyn Baker] {
    &[&svg::SvgBaker, &image::ImageBaker]
}

/// The baker for the asset kind identified by [`AssetKind::id`], or `None` if `id` names no registered baker.
pub fn baker_for_id(id: &str) -> Option<&'static dyn Baker> {
    bakers().iter().copied().find(|b| b.kind().id == id)
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
