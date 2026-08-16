use std::path::PathBuf;

use lsp_types::*;
use ra_ap_ide::TextSize;
use telar_transpiler::SourceMap;

use crate::position::{Section, find_section_at};
use crate::ra::{EmbeddedAnalyzer, RefTarget};
use crate::text::byte_offset;

use super::mapping::reverse_map_rust_refs;
use super::{AnalyzerState, Backend, mark_reload};

/// `[logic]` lines are emitted verbatim under a fixed function-body indent, so an `.rsx` column maps to the
/// generated column by adding this.
const LOGIC_INDENT: u32 = 4;

impl Backend {
    /// Maps an `.rsx` cursor into the generated module and runs `run` against the embedded analyzer on a
    /// blocking thread (the query is synchronous; the load may still be in flight, in which case this yields
    /// `None`). One entry point for both sections, since only the cursor resolution differs — see
    /// [`generated_offset`].
    pub(crate) async fn rust_query<T, F>(
        &self,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
        run: F,
    ) -> Option<T>
    where
        F: FnOnce(&mut EmbeddedAnalyzer, PathBuf, String, TextSize) -> Option<T> + Send + 'static,
        T: Send + 'static,
    {
        let section = find_section_at(&source, pos.line);
        let crate::build_sync::GeneratedTarget {
            path: gen_path,
            code: gen_text,
            map,
        } = crate::build_sync::generated_target(&rsx_path, &source, theme.as_deref())?;
        let offset = generated_offset(section, &source, &gen_text, &map, pos)?;

        let root = crate::build_sync::crate_root(&rsx_path)?;
        self.ensure_loading(root, Some(gen_path.clone()));
        let reload_at = self.reload_at.clone();

        let analyzer = self.analyzer.clone();
        let outgoing = self.outgoing.clone();
        tokio::task::spawn_blocking(move || {
            let lock_at = std::time::Instant::now();
            let mut state = analyzer.lock().ok()?;
            let lock_ms = lock_at.elapsed().as_millis();
            let AnalyzerState::Ready(a) = &mut *state else {
                return None;
            };
            // A generated module the graph doesn't know yet (e.g. a `.rsx` added since load): drop to Idle so the next query reloads the workspace.
            if !a.knows_file(&gen_path) {
                mark_reload(&reload_at);
                return None;
            }
            let ra_at = std::time::Instant::now();
            let result = run(a, gen_path, gen_text, offset);
            // Only surface slow queries — a healthy query is sub-100ms, so anything past 1s flags a regression (e.g. a cold cache) without spamming the output on every keystroke.
            let ra_ms = ra_at.elapsed().as_millis();
            if lock_ms > 1000 || ra_ms > 1000 {
                let section = match section {
                    Section::Logic => "[logic]",
                    _ => "[view]",
                };
                outgoing.log_message(
                    MessageType::INFO,
                    format!(
                        "telar-analyzer: slow {section} query — lock {lock_ms}ms, ra {ra_ms}ms"
                    ),
                );
            }
            result
        })
        .await
        .ok()
        .flatten()
    }

    /// Runs `run` against the embedded analyzer on a blocking thread with no position mapping — for queries that target an offset computed directly in the generated file (component rename probes the generated `fn`/`Props` definitions). Yields `None` while the workspace load is still in flight or the generated module is unknown (a `.rsx` added since load → drop to Idle to reload).
    pub(crate) async fn run_analyzer<T, F>(
        &self,
        gen_path: PathBuf,
        root: PathBuf,
        run: F,
    ) -> Option<T>
    where
        F: FnOnce(&mut EmbeddedAnalyzer) -> Option<T> + Send + 'static,
        T: Send + 'static,
    {
        self.ensure_loading(root, Some(gen_path.clone()));
        let reload_at = self.reload_at.clone();
        let analyzer = self.analyzer.clone();
        tokio::task::spawn_blocking(move || {
            let mut state = analyzer.lock().ok()?;
            let AnalyzerState::Ready(a) = &mut *state else {
                return None;
            };
            if !a.knows_file(&gen_path) {
                mark_reload(&reload_at);
                return None;
            }
            run(a)
        })
        .await
        .ok()
        .flatten()
    }

    /// Find-all-references for the Rust symbol under a `[logic]`/`[view]` cursor, via the embedded analyzer. Returns raw [`RefTarget`]s in generated-file coordinates; the caller reverse-maps them.
    async fn rust_references(
        &self,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
    ) -> Option<Vec<RefTarget>> {
        self.rust_query(rsx_path, source, theme, pos, |a, path, text, offset| {
            a.references_at_offset(&path, text, offset)
        })
        .await
    }

    /// Find-all-references for the Rust symbol under the cursor, reverse-mapped to `.rsx` `Location`s: refs in this file's generated module map back through the line / expr-span maps; refs in real source files pass through verbatim; refs in *other* generated modules are dropped (a file-scoped `[logic]` symbol has none, and a component's cross-component Rust calls are renamed via the dedicated component-rename path instead). Returns `(locations, unmapped)`: the reverse-mapped reference `Location`s plus the count of generated-file references that couldn't be placed (see [`reverse_map_rust_refs`]). Read-only callers ignore `unmapped`; rename refuses when it is non-zero.
    pub(crate) async fn rust_reference_locations(
        &self,
        uri: &Uri,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
    ) -> Option<(Vec<Location>, usize)> {
        let refs = self
            .rust_references(rsx_path.clone(), source.clone(), theme.clone(), pos)
            .await?;
        let target = crate::build_sync::generated_target(&rsx_path, &source, theme.as_deref())?;
        Some(reverse_map_rust_refs(
            refs,
            &target.path,
            &target.code,
            &target.map,
            &source,
            uri,
        ))
    }
}

/// Where the `.rsx` cursor lands inside the generated module, or `None` when no Rust sits under it.
///
/// `[logic]` is emitted verbatim, so the line map places it and the column only shifts by the body indent.
/// `[view]` has no lines of its own in the output — only the verbatim expressions the transpiler copied — so
/// it resolves through the expression-span map, and a cursor outside every span yields `None`, which is what
/// leaves native element/attribute completion in charge. The offset is a UTF-8 char boundary by
/// construction: the fragment is byte-identical in source and output, and the cursor resolves on a boundary.
fn generated_offset(
    section: Section,
    source: &str,
    generated: &str,
    map: &SourceMap,
    pos: Position,
) -> Option<TextSize> {
    let byte = match section {
        Section::Logic => {
            // First generated line that originated from this `.rsx` line.
            let gen_line = map.lines.iter().position(|m| *m == Some(pos.line))? as u32;
            byte_offset(generated, gen_line, pos.character + LOGIC_INDENT)?
        }
        Section::View => {
            let rsx_byte = byte_offset(source, pos.line, pos.character)?;
            // The containing expression span; the inclusive upper bound lets the cursor sit right after the last character (the common completion position, e.g. `count.|`).
            let span = map.exprs.iter().find(|s| {
                rsx_byte >= s.rsx_start as usize && rsx_byte <= (s.rsx_start + s.len) as usize
            })?;
            span.gen_start as usize + (rsx_byte - span.rsx_start as usize)
        }
        _ => return None,
    };
    Some(TextSize::from(byte as u32))
}
