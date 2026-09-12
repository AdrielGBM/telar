//! RSX transpiler: converts a parsed [`RsxDocument`](telar_parser::RsxDocument) AST into compilable Rust source code that depends on `telar::*`.
//!
//! **Producing** the Rust is all this does. **Placing** it — the walk over `src/`, the output paths, the build/asset/catalog artifacts and the handshakes that decide whether one still answers — is `telar-project`, which is what every application's build actually needs and which no longer drags a code generator in behind it.

#![warn(rustdoc::broken_intra_doc_links)]

#[cfg(feature = "transpile")]
mod codegen;
#[cfg(feature = "transpile")]
mod edges;
#[cfg(feature = "transpile")]
mod error;
#[cfg(feature = "transpile")]
mod gradient;
#[cfg(feature = "transpile")]
mod lexer;
#[cfg(feature = "transpile")]
mod package;
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

#[cfg(feature = "transpile")]
pub use codegen::{TranspiledSource, transpile_module_root, transpile_source};
#[cfg(feature = "transpile")]
pub use error::TranspileError;
#[cfg(feature = "transpile")]
pub use package::{
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
pub use theme::resolve_theme_type;

#[cfg(all(test, feature = "transpile"))]
#[path = "lib_test.rs"]
mod tests;
