//! Embedded rust-analyzer (`ra_ap_*`): loads the real workspace into an in-process
//! `RootDatabase` so `.rsx` `[logic]` completion can query the generated Rust
//! synchronously, sidestepping the LSP→disk→rust-analyzer keystroke race.
//!
//! Position mapping (`.rsx` cursor ↔ generated `.rs`) stays our responsibility via
//! the transpiler's line-based source map — rust-analyzer's span machinery only
//! maps real macro expansions, and the generated file is an ordinary `#[path] mod`.

use std::path::{Path, PathBuf};

use lsp_types::{
    CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, Hover, HoverContents,
    MarkupContent, MarkupKind, NumberOrString, ParameterInformation, ParameterLabel, Position,
    Range, SignatureHelp, SignatureInformation,
};
use ra_ap_ide::{
    AnalysisHost, AssistResolveStrategy, CompletionConfig, CompletionFieldsToResolve,
    CompletionItemKind as RaKind, DiagnosticsConfig, FilePosition, FileRange, GotoDefinitionConfig,
    HoverConfig, HoverDocFormat, LineIndex, RaFixtureConfig, Severity, SubstTyLen, SymbolKind,
    TextRange, TextSize,
};
use ra_ap_ide_db::ChangeWithProcMacros;
use ra_ap_ide_db::imports::insert_use::{ImportGranularity, InsertUseConfig, PrefixKind};
use ra_ap_ide_db::line_index::WideEncoding;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, load_workspace_at};
use ra_ap_paths::AbsPathBuf;
use ra_ap_proc_macro_api::ProcMacroClient;
use ra_ap_project_model::{CargoConfig, RustLibSource};
use ra_ap_vfs::{FileId, Vfs, VfsPath};

/// `[logic]` lines are emitted verbatim under a fixed function-body indent, so an
/// `.rsx` column maps to the generated column by adding this. (Generalized away by
/// the byte-span source map, T-S1.)
const LOGIC_INDENT: u32 = 4;

/// A go-to-definition target resolved against the embedded analyzer: the target file's path and the
/// name range within it, in LSP (line, UTF-16 col) coordinates. The backend decides whether the path
/// is a generated `.rsx/build/*.rs` (reverse-mapped to the `.rsx`) or a real file (used verbatim).
pub struct DefinitionTarget {
    pub path: PathBuf,
    pub range: Range,
}

/// An in-process rust-analyzer over the workspace that owns the `.rsx`-generated
/// Rust, fed live via the overlay in each query.
pub struct EmbeddedAnalyzer {
    host: AnalysisHost,
    vfs: Vfs,
    // Dropping the proc-macro client kills the proc-macro server; `app!` would then
    // stop expanding, and rust-analyzer would lose the generated `#[path] mod` files.
    _proc_macro: Option<ProcMacroClient>,
}

impl EmbeddedAnalyzer {
    /// Loads the cargo workspace at `workspace_root` into a fresh database.
    /// Synchronous and slow (runs `cargo metadata` + builds the crate graph), so
    /// callers run it off the LSP's runtime thread.
    pub fn load(workspace_root: &Path) -> anyhow::Result<Self> {
        let cargo_config = CargoConfig {
            sysroot: Some(RustLibSource::Discover),
            ..CargoConfig::default()
        };
        // Proc-macro server is required, not optional: `app!` expansion is how the
        // generated modules are discovered (see design doc, "Invariants & gotchas").
        let load_config = LoadCargoConfig {
            load_out_dirs_from_check: true,
            with_proc_macro_server: ProcMacroServerChoice::Sysroot,
            // Cache priming is done explicitly via `warm()` after load — the `prefill_caches` flag in
            // this version doesn't prime enough (load stays ~2s and the first query still pays ~15s).
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

    /// Forces rust-analyzer to analyze `gen_path`'s crate once (resolving the dependency graph + the
    /// framework's HIR — the ~15s one-time cost), so the first interactive query is fast. salsa caches
    /// the result; a later overlay only re-analyzes the single changed file. Result discarded.
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

    /// `[logic]` completion: overlays the freshly transpiled Rust for `gen_path`, then
    /// queries rust-analyzer at the mapped cursor. `line`/`col` are in the generated
    /// file (`.rsx` line resolved through the source map, column already `+LOGIC_INDENT`).
    pub fn completions_at(
        &mut self,
        gen_path: &Path,
        generated: String,
        line: u32,
        col: u32,
    ) -> Vec<CompletionItem> {
        let Some(offset) = byte_offset(&generated, line, col) else {
            return Vec::new();
        };
        self.completions_at_offset(gen_path, generated, offset)
    }

    /// Completion at an exact byte `offset` in the generated file. The `[view]` path computes the
    /// offset itself (via the expression-span map), so it bypasses the line/column mapping that the
    /// `[logic]` path uses. `offset` must land on a UTF-8 char boundary or rust-analyzer panics.
    pub fn completions_at_offset(
        &mut self,
        gen_path: &Path,
        generated: String,
        offset: TextSize,
    ) -> Vec<CompletionItem> {
        let Some(file_id) = self.file_id(gen_path) else {
            return Vec::new();
        };
        self.overlay(file_id, generated);
        let analysis = self.host.analysis();
        let config = completion_config();
        let pos = FilePosition { file_id, offset };
        let items = analysis
            .completions(&config, pos, None)
            .ok()
            .flatten()
            .unwrap_or_default();
        items
            .into_iter()
            .map(|item| CompletionItem {
                label: item.lookup().to_string(),
                kind: map_completion_kind(item.kind),
                detail: item.detail.clone(),
                ..Default::default()
            })
            .collect()
    }

    /// `[logic]` signature help, mapped to LSP. Same overlay→query path as completion.
    pub fn signature_help_at(
        &mut self,
        gen_path: &Path,
        generated: String,
        line: u32,
        col: u32,
    ) -> Option<SignatureHelp> {
        let offset = byte_offset(&generated, line, col)?;
        self.signature_help_at_offset(gen_path, generated, offset)
    }

    /// Signature help at an exact byte `offset` in the generated file (used by the `[view]` path).
    /// `offset` must land on a UTF-8 char boundary.
    pub fn signature_help_at_offset(
        &mut self,
        gen_path: &Path,
        generated: String,
        offset: TextSize,
    ) -> Option<SignatureHelp> {
        let file_id = self.file_id(gen_path)?;
        self.overlay(file_id, generated);
        let analysis = self.host.analysis();
        let pos = FilePosition { file_id, offset };
        let help = analysis.signature_help(pos).ok().flatten()?;
        let active = help.active_parameter.map(|n| n as u32);
        let parameters = help
            .parameter_labels()
            .map(|p| ParameterInformation {
                label: ParameterLabel::Simple(p.to_string()),
                documentation: None,
            })
            .collect();
        Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label: help.signature.clone(),
                documentation: None,
                parameters: Some(parameters),
                active_parameter: active,
            }],
            active_signature: Some(0),
            active_parameter: active,
        })
    }

    /// `[logic]` hover, mapped to LSP. Same overlay→query path as completion.
    pub fn hover_at(
        &mut self,
        gen_path: &Path,
        generated: String,
        line: u32,
        col: u32,
    ) -> Option<Hover> {
        let offset = byte_offset(&generated, line, col)?;
        self.hover_at_offset(gen_path, generated, offset)
    }

    /// Hover at an exact byte `offset` in the generated file (used by the `[view]` path). `offset`
    /// must land on a UTF-8 char boundary. The range is omitted so the client highlights the hovered
    /// `.rsx` word itself — mapping the generated-file range back is unnecessary for a tooltip.
    pub fn hover_at_offset(
        &mut self,
        gen_path: &Path,
        generated: String,
        offset: TextSize,
    ) -> Option<Hover> {
        let file_id = self.file_id(gen_path)?;
        self.overlay(file_id, generated);
        let analysis = self.host.analysis();
        let range = FileRange {
            file_id,
            range: TextRange::empty(offset),
        };
        let info = analysis.hover(&hover_config(), range).ok().flatten()?;
        let value = info.info.markup.as_str().to_string();
        if value.is_empty() {
            return None;
        }
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: None,
        })
    }

    /// `[logic]` go-to-definition. Same overlay→query path as completion.
    pub fn definition_at(
        &mut self,
        gen_path: &Path,
        generated: String,
        line: u32,
        col: u32,
    ) -> Option<Vec<DefinitionTarget>> {
        let offset = byte_offset(&generated, line, col)?;
        self.definition_at_offset(gen_path, generated, offset)
    }

    /// Definition at an exact byte `offset` in the generated file (used by the `[view]` path).
    /// `offset` must land on a UTF-8 char boundary. Each navigation target is resolved to its file
    /// path (via the `Vfs`) and name range (in that file's coordinates); the backend reverse-maps the
    /// generated `.rsx/build/*.rs` ones back to the `.rsx`. `None` means rust-analyzer found nothing.
    pub fn definition_at_offset(
        &mut self,
        gen_path: &Path,
        generated: String,
        offset: TextSize,
    ) -> Option<Vec<DefinitionTarget>> {
        let file_id = self.file_id(gen_path)?;
        self.overlay(file_id, generated);
        let analysis = self.host.analysis();
        let pos = FilePosition { file_id, offset };
        let nav = analysis
            .goto_definition(pos, &goto_definition_config())
            .ok()
            .flatten()?;
        let mut targets = Vec::new();
        for target in nav.info {
            let Some(path) = self.file_path(target.file_id) else {
                continue;
            };
            let Ok(line_index) = analysis.file_line_index(target.file_id) else {
                continue;
            };
            // Prefer the identifier range (`focus_range`); fall back to the whole item.
            let span = target.focus_range.unwrap_or(target.full_range);
            targets.push(DefinitionTarget {
                path,
                range: Range {
                    start: lsp_position(&line_index, span.start()),
                    end: lsp_position(&line_index, span.end()),
                },
            });
        }
        Some(targets)
    }

    /// rust-analyzer diagnostics for the overlaid generated file, in generated-file coordinates. The
    /// backend reverse-maps each line back onto the `.rsx` via the source map. Only diagnostics whose
    /// range is in the generated file are kept (a cross-file diagnostic has no `.rsx` line to map to).
    pub fn diagnostics(&mut self, gen_path: &Path, generated: String) -> Vec<Diagnostic> {
        let Some(file_id) = self.file_id(gen_path) else {
            return Vec::new();
        };
        self.overlay(file_id, generated);
        let analysis = self.host.analysis();
        let raw = analysis
            .full_diagnostics(&diagnostics_config(), AssistResolveStrategy::None, file_id)
            .unwrap_or_default();
        let Ok(line_index) = analysis.file_line_index(file_id) else {
            return Vec::new();
        };
        raw.into_iter()
            .filter(|d| d.range.file_id == file_id)
            .filter_map(|d| {
                let severity = map_severity(d.severity)?;
                Some(Diagnostic {
                    range: Range {
                        start: lsp_position(&line_index, d.range.range.start()),
                        end: lsp_position(&line_index, d.range.range.end()),
                    },
                    severity: Some(severity),
                    code: Some(NumberOrString::String(d.code.as_str().to_string())),
                    source: Some("rust-analyzer".to_string()),
                    message: d.message,
                    ..Default::default()
                })
            })
            .collect()
    }

    /// Whether `path` is a file the loaded workspace graph knows. `false` for a
    /// generated module added after load (a new `.rsx`), which signals a stale graph.
    pub fn knows_file(&self, path: &Path) -> bool {
        self.file_id(path).is_some()
    }

    /// Resolves a filesystem path to its `FileId`, if the file is part of the loaded
    /// workspace (true for a generated `.rs` reached via `#[path] mod`).
    fn file_id(&self, path: &Path) -> Option<FileId> {
        // `AbsPathBuf` is UTF-8 + absolute: `to_str()` rejects non-UTF-8 and
        // `try_from(&str)` rejects non-absolute, so neither path panics.
        let abs = AbsPathBuf::try_from(path.to_str()?).ok()?;
        self.vfs.file_id(&VfsPath::from(abs)).map(|(id, _)| id)
    }

    /// Resolves a `FileId` back to its filesystem path via the `Vfs` (the analyzer's authority on
    /// file_id↔path), for mapping navigation targets to LSP `Location`s.
    fn file_path(&self, file_id: FileId) -> Option<PathBuf> {
        let abs = self.vfs.file_path(file_id).as_path()?;
        let path: &Path = abs.as_ref();
        Some(path.to_path_buf())
    }

    /// Overlays new text for an already-known `FileId`, synchronously updating the
    /// salsa inputs. The analysis snapshot is created only after this returns, so it
    /// never blocks `apply_change` (which waits for every live snapshot to drop).
    fn overlay(&mut self, file_id: FileId, text: String) {
        let mut change = ChangeWithProcMacros::default();
        change.change_file(file_id, Some(text));
        self.host.apply_change(change);
    }
}

/// Column is `rsx_col + LOGIC_INDENT`; exposed so the backend mirrors the same offset.
pub const fn logic_indent() -> u32 {
    LOGIC_INDENT
}

/// Byte offset of `(line, utf16_col)` within `text`, always landing on a UTF-8 char
/// boundary. The column is UTF-16 (LSP convention); converting it byte-wise would point
/// mid-character on multi-byte text and make rust-analyzer's completion panic
/// ("start of range should be a character boundary") when it inserts its synthetic marker.
fn byte_offset(text: &str, line: u32, utf16_col: u32) -> Option<TextSize> {
    let mut line_start = 0usize;
    for (i, current) in text.split_inclusive('\n').enumerate() {
        if i as u32 == line {
            let content = current.strip_suffix('\n').unwrap_or(current);
            let mut remaining = utf16_col;
            let mut byte = 0usize;
            for ch in content.chars() {
                let width = ch.len_utf16() as u32;
                if remaining < width {
                    break;
                }
                remaining -= width;
                byte += ch.len_utf8();
            }
            return Some(TextSize::from((line_start + byte) as u32));
        }
        line_start += current.len();
    }
    None
}

/// A conservative completion config: no fly-imports / snippets / term-search, so the
/// sub-config surface (and version fragility) stays minimal.
fn completion_config() -> CompletionConfig<'static> {
    CompletionConfig {
        enable_postfix_completions: true,
        enable_imports_on_the_fly: false,
        enable_self_on_the_fly: true,
        enable_auto_iter: false,
        enable_auto_await: false,
        enable_private_editable: false,
        enable_term_search: false,
        term_search_fuel: 0,
        full_function_signatures: false,
        callable: None,
        add_colons_to_module: true,
        add_semicolon_to_unit: false,
        snippet_cap: None,
        insert_use: insert_use_config(),
        prefer_no_std: false,
        prefer_prelude: true,
        prefer_absolute: false,
        snippets: Vec::new(),
        limit: None,
        fields_to_resolve: CompletionFieldsToResolve::empty(),
        exclude_flyimport: Vec::new(),
        exclude_traits: &[],
        ra_fixture: RaFixtureConfig::default(),
    }
}

/// Shared import config for completion + diagnostics; both need the same (conservative) settings.
fn insert_use_config() -> InsertUseConfig {
    InsertUseConfig {
        granularity: ImportGranularity::Crate,
        enforce_granularity: false,
        prefix_kind: PrefixKind::Plain,
        group: false,
        skip_glob_imports: false,
    }
}

/// A conservative hover config: markdown on, doc links off (the client can't resolve rust-analyzer's
/// generated-file URLs), no memory-layout block, no field/variant caps.
fn hover_config() -> HoverConfig<'static> {
    HoverConfig {
        links_in_hover: false,
        memory_layout: None,
        documentation: true,
        keywords: true,
        format: HoverDocFormat::Markdown,
        max_trait_assoc_items_count: None,
        max_fields_count: None,
        max_enum_variants_count: None,
        max_subst_ty_len: SubstTyLen::Unlimited,
        show_drop_glue: false,
        ra_fixture: RaFixtureConfig::default(),
    }
}

fn goto_definition_config() -> GotoDefinitionConfig<'static> {
    GotoDefinitionConfig {
        ra_fixture: RaFixtureConfig::default(),
    }
}

/// Diagnostics config: proc macros on (the `app!`-generated modules are only visible through
/// expansion). Experimental diagnostics stay OFF: the type-inference ones (unresolved-name,
/// type-mismatch) cascade — a single typo poisons inference for the whole tightly-coupled view and
/// floods the `.rsx` with derived "type annotations needed" errors. Syntax + stable name-resolution
/// diagnostics still surface; authoritative semantic errors belong to a future `cargo check` flycheck.
/// Style lints off, no term search. No `Default` impl exists, so every field is set explicitly.
fn diagnostics_config() -> DiagnosticsConfig {
    DiagnosticsConfig {
        enabled: true,
        proc_macros_enabled: true,
        proc_attr_macros_enabled: true,
        disable_experimental: true,
        disabled: Default::default(),
        expr_fill_default: Default::default(),
        style_lints: false,
        snippet_cap: None,
        insert_use: insert_use_config(),
        prefer_no_std: false,
        prefer_prelude: true,
        prefer_absolute: false,
        term_search_fuel: 0,
        term_search_borrowck: false,
        show_rename_conflicts: false,
    }
}

/// `None` drops a suppressed (`#[allow]`-ed) diagnostic; everything else maps to its LSP severity.
fn map_severity(severity: Severity) -> Option<DiagnosticSeverity> {
    Some(match severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::WeakWarning => DiagnosticSeverity::HINT,
        Severity::Allow => return None,
    })
}

/// A generated-file byte `offset` → LSP `(line, UTF-16 col)`, via the file's `LineIndex`. rust-analyzer
/// columns are UTF-8; LSP wants UTF-16, so `to_wide` converts. An invalid offset collapses to the file
/// start rather than panicking (the release binary is `panic="abort"`).
fn lsp_position(line_index: &LineIndex, offset: TextSize) -> Position {
    let Some(line_col) = line_index.try_line_col(offset) else {
        return Position {
            line: 0,
            character: 0,
        };
    };
    match line_index.to_wide(WideEncoding::Utf16, line_col) {
        Some(wide) => Position {
            line: wide.line,
            character: wide.col,
        },
        None => Position {
            line: line_col.line,
            character: line_col.col,
        },
    }
}

fn map_completion_kind(kind: RaKind) -> Option<CompletionItemKind> {
    use CompletionItemKind as L;
    Some(match kind {
        RaKind::SymbolKind(sk) => map_symbol_kind(sk),
        RaKind::Binding => L::VARIABLE,
        RaKind::BuiltinType => L::CLASS,
        RaKind::InferredType => L::VALUE,
        RaKind::Keyword => L::KEYWORD,
        RaKind::Snippet => L::SNIPPET,
        RaKind::UnresolvedReference => L::REFERENCE,
        RaKind::Expression => L::VALUE,
    })
}

fn map_symbol_kind(kind: SymbolKind) -> CompletionItemKind {
    use CompletionItemKind as L;
    match kind {
        SymbolKind::Function | SymbolKind::Macro => L::FUNCTION,
        SymbolKind::Method => L::METHOD,
        SymbolKind::Struct => L::STRUCT,
        SymbolKind::Enum => L::ENUM,
        SymbolKind::Variant => L::ENUM_MEMBER,
        SymbolKind::Field => L::FIELD,
        SymbolKind::Module => L::MODULE,
        SymbolKind::Trait => L::INTERFACE,
        SymbolKind::Const | SymbolKind::Static => L::CONSTANT,
        SymbolKind::TypeAlias => L::CLASS,
        SymbolKind::Local | SymbolKind::ValueParam | SymbolKind::SelfParam => L::VARIABLE,
        _ => L::TEXT,
    }
}
