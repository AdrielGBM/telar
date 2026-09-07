//! RSX transpiler: converts a parsed [`RsxDocument`](telar_parser::RsxDocument) AST into compilable Rust source code that depends on `telar::*`.
//!
//! Two halves, split by the `transpile` feature. **Placing** what a transpile produced — the walk over `src/`, the output paths, the build/asset/catalog artifacts and the handshakes that decide whether one still answers — is what every build needs. **Producing** it is what only `cargo telar transpile`, the editor and the golden harness need, and it is the half that carries the parser and the code generator. A project that builds through the CLI never compiles the second one.

#![warn(rustdoc::broken_intra_doc_links)]

mod assets;
mod catalog;
#[cfg(feature = "transpile")]
mod codegen;
mod discovery;
#[cfg(feature = "transpile")]
mod edges;
#[cfg(feature = "transpile")]
mod error;
#[cfg(feature = "transpile")]
mod gradient;
pub mod naming;
mod paths;
mod project;
#[cfg(feature = "transpile")]
mod registry;
#[cfg(feature = "transpile")]
mod rust;
#[cfg(feature = "transpile")]
mod signal_scan;
#[cfg(feature = "transpile")]
mod source_map;
#[cfg(feature = "transpile")]
mod style;
mod theme;
#[cfg(feature = "transpile")]
mod transition;
#[cfg(feature = "transpile")]
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
#[cfg(feature = "transpile")]
pub use codegen::{TranspiledSource, transpile_source};
pub use discovery::{
    assets_root, auto_modules_enabled, collect_files_by_ext, component_name, discover_rust_modules,
    find_rsx_files, find_rsx_files_in_tree, prune_stale_generated, read_rsx_section,
    relative_output_path,
};
#[cfg(feature = "transpile")]
pub use error::TranspileError;
pub use paths::{
    find_ancestor_dir, find_telar_root, find_workspace_root, resolve_telar_version,
    write_if_changed_atomic,
};
pub use project::{
    BUILD_ARTIFACT_FORMAT, BuildEntry, BuildFlavour, BuildIndex, generated_dir, read_build_index,
    write_build_index,
};
#[cfg(feature = "transpile")]
pub use project::{
    GeneratedFile, PackageError, PackageOptions, build_index, transpile_package, write_package,
};
#[cfg(feature = "transpile")]
pub use registry::{
    AttrSpec, ROLE_VALUES, ValueKind, attr_doc, attr_spec, builtin_tags, color_attr_keys,
    color_keywords, is_builtin_tag, is_control_flow_keyword, keyword_color_rgba, layout_attr_keys,
    role_values, role_variant, tag_attr_keys, tag_attr_specs, value_kind,
};
#[cfg(feature = "transpile")]
pub use signal_scan::{SignalInfo, scan_effects, scan_locals, scan_signals};
#[cfg(feature = "transpile")]
pub use source_map::{ExprSpan, RsxSpan, SourceMap, nth_line};
pub use theme::{normalize_theme_path, resolve_theme_type, theme_type_in_config};

#[cfg(all(test, feature = "transpile"))]
#[path = "lib_test.rs"]
mod tests;
