// RSX bundler: drives the parse → transpile → compile pipeline for `.rsx` projects.

use std::path::{Path, PathBuf};

/// Finds all `.rsx` files recursively in a directory.
fn find_rsx_files(dir: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(find_rsx_files(&path));
            } else if path.extension().map(|e| e == "rsx").unwrap_or(false) {
                result.push(path);
            }
        }
    }
    result.sort();
    result
}

/// Derives a snake_case component name from the file stem.
fn component_name(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

#[derive(Debug)]
pub enum BundleError {
    Io(std::io::Error),
    Parse(rsx_parser::ParseError),
    Transpile(rsx_transpiler::TranspileError),
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::Parse(e) => write!(f, "parse error: {e}"),
            Self::Transpile(e) => write!(f, "transpile error: {e}"),
        }
    }
}

impl std::error::Error for BundleError {}

impl From<std::io::Error> for BundleError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<rsx_parser::ParseError> for BundleError {
    fn from(e: rsx_parser::ParseError) -> Self {
        Self::Parse(e)
    }
}

impl From<rsx_transpiler::TranspileError> for BundleError {
    fn from(e: rsx_transpiler::TranspileError) -> Self {
        Self::Transpile(e)
    }
}

pub struct BundleConfig {
    /// Source directory to scan for `.rsx` files (typically `src/`).
    pub src_dir: PathBuf,
    /// Output directory for generated `.rs` files (typically `OUT_DIR`).
    pub out_dir: PathBuf,
    /// When true, emits `cargo:rerun-if-changed=` lines for each `.rsx` file.
    pub emit_rerun: bool,
}

/// Transpiles all `.rsx` files found under `src_dir`, writes them to `out_dir`,
/// and generates an `rsx_components.rs` index that `include!`s each one.
pub fn bundle(config: BundleConfig) -> Result<(), BundleError> {
    let rsx_files = find_rsx_files(&config.src_dir);
    let mut includes = String::new();
    let mut preview_components: Vec<(String, Vec<String>)> = Vec::new();

    for rsx_file in &rsx_files {
        let source = std::fs::read_to_string(rsx_file)?;
        let name = component_name(rsx_file);

        let result =
            rsx_transpiler::transpile_source(&source, &name).map_err(BundleError::Transpile)?;

        if !result.preview_names.is_empty() {
            preview_components.push((name.clone(), result.preview_names.clone()));
        }

        let out_filename = format!("{name}_rsx.rs");
        let out_path = config.out_dir.join(&out_filename);
        std::fs::write(&out_path, &result.rust_code)?;

        includes.push_str(&format!(
            "include!(concat!(env!(\"OUT_DIR\"), \"/{out_filename}\"));\n"
        ));

        if config.emit_rerun {
            println!("cargo:rerun-if-changed={}", rsx_file.display());
        }
    }

    let mut index = includes;
    index.push('\n');
    index.push_str("pub fn rsx_all_preview_entries() -> Vec<::rsx::PreviewEntry> {\n");
    index.push_str("    let mut entries = Vec::new();\n");
    for (comp_name, _names) in &preview_components {
        let const_name = format!(
            "{}_PREVIEW_ENTRIES",
            comp_name.to_uppercase().replace('-', "_")
        );
        index.push_str(&format!("    entries.extend_from_slice({const_name});\n"));
    }
    index.push_str("    entries\n");
    index.push_str("}\n");

    let index_path = config.out_dir.join("rsx_components.rs");
    std::fs::write(index_path, index)?;

    Ok(())
}

/// Convenience entry point for use from a `build.rs` script; reads `OUT_DIR` from the environment.
pub fn build() -> Result<(), BundleError> {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR not set"));
    let src_dir = PathBuf::from("src");

    println!("cargo:rerun-if-changed=build.rs");

    bundle(BundleConfig {
        src_dir,
        out_dir,
        emit_rerun: true,
    })
}
