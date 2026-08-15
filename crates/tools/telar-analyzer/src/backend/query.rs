use std::path::PathBuf;

use lsp_types::*;
use ra_ap_ide::TextSize;

use crate::position::{Section, find_section_at};
use crate::ra::{EmbeddedAnalyzer, RefTarget};
use crate::text::byte_offset;

use super::mapping::reverse_map_rust_refs;
use super::{AnalyzerState, Backend, mark_reload};

impl Backend {
    /// Maps a `[logic]` cursor into the generated module and runs `run` against the embedded analyzer on a blocking thread (the query is synchronous; the load may still be in flight, in which case this yields `None`).
    pub(crate) async fn logic_query<T, F>(
        &self,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
        run: F,
    ) -> Option<T>
    where
        F: FnOnce(&mut EmbeddedAnalyzer, PathBuf, String, u32, u32) -> Option<T> + Send + 'static,
        T: Send + 'static,
    {
        let crate::build_sync::GeneratedTarget {
            path: gen_path,
            code: gen_text,
            map,
            ..
        } = crate::build_sync::generated_target(&rsx_path, &source, theme.as_deref())?;
        // First generated line that originated from this `.rsx` line.
        let gen_line = map.lines.iter().position(|m| *m == Some(pos.line))? as u32;
        let gen_col = pos.character + crate::ra::logic_indent();

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
            let result = run(a, gen_path, gen_text, gen_line, gen_col);
            // Only surface slow queries — a healthy query is sub-100ms, so anything past 1s flags a regression (e.g. a cold cache) without spamming the output on every keystroke.
            let ra_ms = ra_at.elapsed().as_millis();
            if lock_ms > 1000 || ra_ms > 1000 {
                outgoing.log_message(
                    MessageType::INFO,
                    format!("telar-analyzer: slow [logic] query — lock {lock_ms}ms, ra {ra_ms}ms"),
                );
            }
            result
        })
        .await
        .ok()
        .flatten()
    }

    /// Maps a `[view]` cursor into the generated module via the transpiler's expression-span map and runs `run` at the resulting byte offset. Returns `None` (so native element/attr completion is preserved) when the cursor sits outside every verbatim `[view]` expression. The generated offset is a UTF-8 char boundary by construction: the fragment is byte-identical in source and output, and the `.rsx` cursor is resolved on a char boundary.
    pub(crate) async fn view_query<T, F>(
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
        let crate::build_sync::GeneratedTarget {
            path: gen_path,
            code: gen_text,
            map,
        } = crate::build_sync::generated_target(&rsx_path, &source, theme.as_deref())?;
        let rsx_byte = byte_offset(&source, pos.line, pos.character)?;
        // The containing expression span; the inclusive upper bound lets the cursor sit right after the last character (the common completion position, e.g. `count.|`).
        let span = map.exprs.iter().find(|s| {
            rsx_byte >= s.rsx_start as usize && rsx_byte <= (s.rsx_start + s.len) as usize
        })?;
        let gen_offset = span.gen_start as usize + (rsx_byte - span.rsx_start as usize);
        let offset = TextSize::from(gen_offset as u32);

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
            if !a.knows_file(&gen_path) {
                mark_reload(&reload_at);
                return None;
            }
            let ra_at = std::time::Instant::now();
            let result = run(a, gen_path, gen_text, offset);
            let ra_ms = ra_at.elapsed().as_millis();
            if lock_ms > 1000 || ra_ms > 1000 {
                outgoing.log_message(
                    MessageType::INFO,
                    format!("telar-analyzer: slow [view] query — lock {lock_ms}ms, ra {ra_ms}ms"),
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
        match find_section_at(&source, pos.line) {
            Section::Logic => {
                self.logic_query(rsx_path, source, theme, pos, |a, path, text, line, col| {
                    a.references_at(&path, text, line, col)
                })
                .await
            }
            Section::View => {
                self.view_query(rsx_path, source, theme, pos, |a, path, text, offset| {
                    a.references_at_offset(&path, text, offset)
                })
                .await
            }
            _ => None,
        }
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
