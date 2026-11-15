//! One [`Baker`] per registered [`AssetKind`], turning a `.rsx` `src:"path"` asset's raw bytes into the Rust
//! source expression the transpiler embeds for it.
//!
//! This is the machinery both build-time consumers share: `cargo-telar` bakes assets while building an app,
//! and `telar-analyzer` bakes them on its own so the IDE shows no errors for a `src:"…"` the app itself hasn't
//! built yet. Neither depends on the other, so the baking logic lives here instead of in either binary — and
//! instead of in `telar-transpiler`, which every app compiles and which would otherwise carry `usvg`/`resvg`/
//! `image` (~66 host crates) into a project that never bakes an asset.
//!
//! Adding a kind — fonts, shaders, audio — is a variant of [`AssetKind`] plus one [`Baker`] impl here, which
//! [`baker_for`] must then name; nothing in the transpiler, the macro, or a CLI needs to change to pick it up.
//!
//! A kind a plugin's component names by id rather than by path — `icon name:"mdi:home"` in `.rsx`, or `icon!("mdi:home")` in Rust — also needs its ids
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
mod macro_ids;
mod package;
mod problem;
mod svg;
mod web_image;

pub use catalog::{CatalogReport, bake_catalog, catalog_keys, parse_catalog, to_source};
pub use icon_dependencies::{CrateIcons, DependencyGraph, IconDependency, read_library_icons};
pub use icons::{ICONS_RECORD_FILENAME, IconRecord, RecordedIcon, RecordedSet, read_icon_record};
pub use ids::{IdRef, collect_id_refs};
pub use package::{BakeReport, bake_package, bake_package_with, collect_asset_refs};
pub use problem::{AssetRef, BakeProblem, Origin, ProblemKind, Problems, Severity};
pub use telar_project::ICONS_NOTICE_FILENAME;
pub use web_image::{WEB_IMAGES_DIR, WebImage, WebImageFile, web_image};

/// Converts one registered [`AssetKind`]'s raw file bytes into the Rust source expression that reconstructs
/// the equivalent runtime data with no baking dependency left in the compiled app.
///
/// `bytes` is always the asset's raw file content, whatever its kind: a `Baker` for a text format (SVG) is
/// responsible for its own UTF-8 decoding. Keeping that conversion on the impl, not the caller, is what lets
/// [`baker_for`] answer one uniform trait object, with no caller dispatching on kind to pick `&str` vs `&[u8]`.
pub trait Baker {
    /// Bakes `bytes` into a Rust expression, or an error message describing why they could not be baked.
    fn bake(&self, bytes: &[u8]) -> Result<String, String>;
}

pub fn baker_for(kind: AssetKind) -> &'static dyn Baker {
    match kind {
        AssetKind::Svg => &svg::SvgBaker,
        AssetKind::Image => &image::ImageBaker,
        AssetKind::Icon => &svg::IconBaker,
    }
}
