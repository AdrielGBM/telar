//! RSX transpiler: converts a parsed [`RsxDocument`](telar_parser::RsxDocument) AST into compilable Rust source code that depends on `telar::*`.

#![warn(rustdoc::broken_intra_doc_links)]

mod assets;
mod catalog;
mod codegen;
mod discovery;
mod edges;
mod error;
mod gradient;
pub mod naming;
mod paths;
mod project;
mod registry;
mod rust;
mod signal_scan;
mod source_map;
mod style;
mod theme;
mod transition;
mod view;

pub use assets::{
    ASSET_ARTIFACT_FORMAT, ASSET_KINDS, ASSETS_INDEX_FILENAME, ASSETS_MODULE,
    ASSETS_SOURCE_FILENAME, ArtifactHandshake, AssetContext, AssetEntry, AssetIndex, AssetKind,
    BakedAsset, GeneratedAssets, asset_kind_for_id, asset_kind_for_tag, check_artifact,
    content_hash, generate_assets, read_index, static_name_for_path, write_generated,
};
pub use catalog::{
    CATALOG_ARTIFACT_FORMAT, CATALOG_INDEX_FILENAME, CATALOG_SOURCE_FILENAME, CatalogContext,
    CatalogEntry, CatalogIndex, CatalogSourceFile, I18N_CATALOG_PATH, I18N_MODULE,
    read_catalog_index,
};
pub use codegen::{TranspiledSource, transpile_source};
pub use discovery::{
    assets_root, auto_modules_enabled, collect_files_by_ext, component_name, discover_rust_modules,
    find_rsx_files, find_rsx_files_in_tree, prune_stale_generated, read_rsx_section,
    relative_output_path,
};
pub use error::TranspileError;
pub use paths::{find_ancestor_dir, find_telar_root, find_workspace_root, write_if_changed_atomic};
pub use project::{
    BUILD_ARTIFACT_FORMAT, BuildEntry, BuildFlavour, BuildIndex, GeneratedFile, PackageError,
    PackageOptions, build_index, generated_dir, read_build_index, transpile_package,
    write_build_index, write_package,
};
pub use registry::{
    AttrSpec, ROLE_VALUES, ValueKind, attr_doc, attr_spec, builtin_tags, color_attr_keys,
    color_keywords, is_builtin_tag, is_control_flow_keyword, keyword_color_rgba, layout_attr_keys,
    role_values, role_variant, tag_attr_keys, tag_attr_specs, value_kind,
};
pub use signal_scan::{SignalInfo, scan_effects, scan_locals, scan_signals};
pub use source_map::{ExprSpan, RsxSpan, SourceMap, nth_line};
pub use theme::{normalize_theme_path, resolve_theme_type, theme_type_in_config};

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
