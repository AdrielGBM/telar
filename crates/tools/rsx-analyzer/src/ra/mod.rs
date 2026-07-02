//! Embedded rust-analyzer (`ra_ap_*`): loads the real workspace into an in-process `RootDatabase` so `.rsx` `[logic]` completion can query the generated Rust synchronously, sidestepping the LSP→disk→rust-analyzer keystroke race.
//!
//! Position mapping (`.rsx` cursor ↔ generated `.rs`) stays our responsibility via the transpiler's line-based source map — rust-analyzer's span machinery only maps real macro expansions, and the generated file is an ordinary `#[path] mod`.

use std::path::{Path, PathBuf};

use lsp_types::{InlayHintKind, Range};
use ra_ap_ide::{AnalysisHost, AssistResolveStrategy};
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, load_workspace_at};
use ra_ap_proc_macro_api::ProcMacroClient;
use ra_ap_project_model::{CargoConfig, RustLibSource};
use ra_ap_vfs::Vfs;

use config::diagnostics_config;

mod config;
mod diagnostics;
mod mapping;
mod queries;
mod vfs;

/// `[logic]` lines are emitted verbatim under a fixed function-body indent, so an `.rsx` column maps to the generated column by adding this. (Generalized away by the byte-span source map, T-S1.)
const LOGIC_INDENT: u32 = 4;

/// A go-to-definition target resolved against the embedded analyzer: the target file's path and the name range within it, in LSP (line, UTF-16 col) coordinates. The backend decides whether the path is a generated `.rsx/build/*.rs` (reverse-mapped to the `.rsx`) or a real file (used verbatim).
pub struct DefinitionTarget {
    pub path: PathBuf,
    pub range: Range,
}

/// One reference to the symbol under the cursor, found by the embedded analyzer (declaration site included). Carries both the byte span (so the backend can reverse-map `[view]` verbatim expressions through the byte-span source map) and the same span in LSP coordinates (used verbatim for real source files). The backend reverse-maps generated `.rsx/build/*.rs` paths back onto their `.rsx`.
pub struct RefTarget {
    pub path: PathBuf,
    pub byte_start: u32,
    pub byte_end: u32,
    pub range: Range,
}

/// One inlay hint from the embedded analyzer, anchored in generated-file `(line, UTF-16 col)`. The backend reverse-maps it onto the `.rsx` and keeps only `[logic]`-origin hints.
pub struct InlayHintRaw {
    pub line: u32,
    pub col: u32,
    pub pad_left: bool,
    pub pad_right: bool,
    pub kind: Option<InlayHintKind>,
    pub label: String,
}

/// An in-process rust-analyzer over the workspace that owns the `.rsx`-generated Rust, fed live via the overlay in each query.
pub struct EmbeddedAnalyzer {
    host: AnalysisHost,
    vfs: Vfs,
    // Dropping the proc-macro client kills the proc-macro server; `app!` would then stop expanding, and rust-analyzer would lose the generated `#[path] mod` files.
    _proc_macro: Option<ProcMacroClient>,
}

impl EmbeddedAnalyzer {
    /// Loads the cargo workspace at `workspace_root` into a fresh database. Synchronous and slow (runs `cargo metadata` + builds the crate graph), so callers run it off the LSP's runtime thread.
    pub fn load(workspace_root: &Path) -> anyhow::Result<Self> {
        let cargo_config = CargoConfig {
            sysroot: Some(RustLibSource::Discover),
            ..CargoConfig::default()
        };
        // Proc-macro server is required, not optional: `app!` expansion is how the generated modules are discovered (see design doc, "Invariants & gotchas").
        let load_config = LoadCargoConfig {
            load_out_dirs_from_check: true,
            with_proc_macro_server: ProcMacroServerChoice::Sysroot,
            // Cache priming is done explicitly via `warm()` after load — the `prefill_caches` flag in this version doesn't prime enough (load stays ~2s and the first query still pays ~15s).
            prefill_caches: false,
            num_worker_threads: 0,
            proc_macro_processes: 1,
        };
        let (db, vfs, proc_macro) =
            load_workspace_at(workspace_root, &cargo_config, &load_config, &|_| {})?;
        Ok(Self {
            host: AnalysisHost::with_database(db),
            vfs,
            _proc_macro: proc_macro,
        })
    }

    /// Forces rust-analyzer to analyze `gen_path`'s crate once (resolving the dependency graph + the framework's HIR — the ~15s one-time cost), so the first interactive query is fast. salsa caches the result; a later overlay only re-analyzes the single changed file. Result discarded.
    pub fn warm(&self, gen_path: &Path) {
        let Some(file_id) = self.file_id(gen_path) else {
            return;
        };
        let _ = self.host.analysis().full_diagnostics(
            &diagnostics_config(),
            AssistResolveStrategy::None,
            file_id,
        );
    }
}

/// Column is `rsx_col + LOGIC_INDENT`; exposed so the backend mirrors the same offset.
pub const fn logic_indent() -> u32 {
    LOGIC_INDENT
}
