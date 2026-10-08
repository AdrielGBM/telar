//! What a Telar project *is*, for everything that has to read one without compiling any `.rsx`.
//!
//! Where the sources are and where their output goes; the naming convention the generated code follows; the baked asset and catalog artifacts, and the index describing what a transpile left behind. None of it parses markup, and all of it is what a build needs once `cargo telar transpile` has already run.
//!
//! Split out of `telar-transpiler` because every application compiled this: `telar` depends on `telar-macros`, which reads these — the walk over `src/`, the output paths, the handshakes that say whether an artifact still answers — and got a fifteen-thousand-line transpiler in the build graph to do it. The transpiler proper now depends on this crate, and nothing that only places output depends on the transpiler.

#![warn(rustdoc::broken_intra_doc_links)]

mod assets;
mod build;
mod cargo_manifest;
mod catalog;
mod dependencies;
mod discovery;
mod fonts;
mod icons;
mod library;
mod manifest;
pub mod naming;
mod paths;
mod prelude;
mod prerender;
mod theme;
pub mod theme_tokens;
mod web;

pub use assets::{
    ASSET_ARTIFACT_FORMAT, ASSET_KINDS, ASSETS_INDEX_FILENAME, ASSETS_MODULE,
    ASSETS_SOURCE_FILENAME, ArtifactHandshake, AssetContext, AssetEntry, AssetIndex, AssetKind,
    BakedAsset, BakedId, ComponentAsset, GeneratedAssets, IdBaking, asset_kind_for_component,
    asset_kind_for_id, asset_kind_for_tag, baked_init_expr, check_artifact, content_hash,
    generate_assets, read_index, static_name_for_path, write_generated,
};
pub use build::{
    BUILD_ARTIFACT_FORMAT, BuildEntry, BuildFailure, BuildFlavour, BuildIndex, clear_build_failure,
    generated_dir, read_build_failure, read_build_index, relative_source, write_build_failure,
    write_build_index,
};
pub use catalog::{
    CATALOG_ARTIFACT_FORMAT, CATALOG_INDEX_FILENAME, CATALOG_SOURCE_FILENAME, CatalogContext,
    CatalogEntry, CatalogIndex, CatalogSourceFile, I18N_CATALOG_PATH, I18N_MODULE,
    read_catalog_index,
};
pub use dependencies::{DEVTOOLS_PACKAGE, declares_optional_dependency};
pub use discovery::{
    MODULE_CHILDREN_FILENAME, MODULE_ROOT_FILENAME, MODULE_TREE_DIR, ModuleTree, SITE_DIR,
    assets_root, collect_files_by_ext, component_name, find_rsx_files, find_rsx_files_in_tree,
    invokes_placement_macro, is_generated_output, is_module_root, is_previews_file,
    placement_sites, prune_stale_generated, prune_stale_sites, relative_output_path,
    site_include_path, source_for_generated, stray_placement_files,
};
pub use fonts::{
    FontDeclaration, FontDisplay, FontFormat, FontStyleDeclaration, WeightDeclaration,
};
pub use icons::{
    ICONS_NOTICE_FILENAME, IconLicensesSection, IconMode, IconsSection, UnlistedLicense,
    icon_notice_file,
};
pub use library::{
    ICONS_LIBRARY_RECORD_FILENAME, library_files, library_include, library_include_covers,
};
pub use manifest::{
    DevSection, I18nSection, MANIFEST_FILENAME, ManifestError, PrerenderSection, RendererBackend,
    TelarManifest, TelarSection, WebSection, WindowSection,
};
pub use paths::{
    cargo_metadata, find_ancestor_dir, find_package_root, find_target_dir, find_telar_root,
    find_workspace_root, resolve_telar_version, telar_version_in, write_if_changed_atomic,
};
pub use prelude::{
    PreludeDeclaration, PreludeEntry, PreludeProblem, prelude_declarations, prelude_problems,
    resolve_prelude,
};
pub use prerender::{
    DEFAULT_SURFACE, PRERENDER_ENV, PageLocation, PageRequest, Preferences, PrerenderRequest,
    PrerenderState, PrerenderedPage, STATE_ELEMENT_ID, STATE_VERSION, Surface,
};
pub use theme::{normalize_theme_path, theme_type_in_config};
pub use web::{OgImage, SchemeColors, ThemeColor, WebHost, is_absolute_url};
