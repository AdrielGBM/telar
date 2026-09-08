//! Asking rust-analyzer about a position, with the `.rsx` cursor mapped into the generated Rust.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use lsp_types::*;
use telar_transpiler::SourceMap;

use crate::position::{Section, find_section_at};
use crate::ra::{Analyzer, RefTarget};
use crate::text::byte_offset;

use super::Backend;
use super::mapping::reverse_map_rust_refs;

/// `[logic]` lines are emitted verbatim under a fixed function-body indent, so an `.rsx` column maps to the generated column by adding this.
const LOGIC_INDENT: u32 = 4;

/// Past this, a query is slow enough to be worth a line in the output without spamming it.
const SLOW_QUERY: u128 = 1000;

impl Backend {
    /// Maps an `.rsx` cursor into the generated module and runs `run` against rust-analyzer. One entry point for both sections, since only the cursor resolution differs — see [`generated_offset`].
    pub(crate) async fn rust_query<T, F, Fut>(
        &self,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
        run: F,
    ) -> Option<T>
    where
        F: FnOnce(Arc<Analyzer>, PathBuf, String, usize) -> Fut,
        Fut: Future<Output = Option<T>>,
    {
        self.rust_query_at(rsx_path, source, theme, pos, generated_offset, run)
            .await
    }

    /// The same, with the cursor resolved by `locate` instead of by section. One entry point, because only the resolution differs — an attribute *key* has no expression span of its own, so it maps to the props builder that carries its setter rather than to itself.
    pub(crate) async fn rust_query_at<T, F, Fut, L>(
        &self,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
        locate: L,
        run: F,
    ) -> Option<T>
    where
        F: FnOnce(Arc<Analyzer>, PathBuf, String, usize) -> Fut,
        Fut: Future<Output = Option<T>>,
        L: FnOnce(Section, &str, &str, &SourceMap, Position) -> Option<usize>,
    {
        let section = find_section_at(&source, pos.line);
        let crate::build_sync::GeneratedTarget {
            path: gen_path,
            code: gen_text,
            map,
        } = crate::build_sync::generated_target(&rsx_path, &source, theme.as_deref())?;
        let offset = locate(section, &source, &gen_text, &map, pos)?;

        let root = crate::build_sync::crate_root(&rsx_path)?;
        let analyzer = self.analyzer(root).await?;

        let started = Instant::now();
        let result = run(analyzer, gen_path, gen_text, offset).await;
        let elapsed = started.elapsed().as_millis();
        if elapsed > SLOW_QUERY {
            let section = match section {
                Section::Logic => "[logic]",
                _ => "[view]",
            };
            self.outgoing().log_message(
                MessageType::INFO,
                format!("telar-analyzer: slow {section} query — {elapsed}ms"),
            );
        }
        result
    }

    /// Runs `run` against rust-analyzer with no position mapping — for queries that target an offset computed directly in the generated file (component rename probes the generated `fn`/`Props` definitions).
    pub(crate) async fn run_analyzer<T, F, Fut>(&self, root: PathBuf, run: F) -> Option<T>
    where
        F: FnOnce(Arc<Analyzer>) -> Fut,
        Fut: Future<Output = Option<T>>,
    {
        let analyzer = self.analyzer(root).await?;
        run(analyzer).await
    }

    /// Find-all-references for the Rust symbol under a `[logic]`/`[view]` cursor. Returns raw [`RefTarget`]s in generated-file coordinates; the caller reverse-maps them.
    async fn rust_references(
        &self,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
    ) -> Option<Vec<RefTarget>> {
        self.rust_query(
            rsx_path,
            source,
            theme,
            pos,
            |a, path, text, offset| async move { a.references(&path, &text, offset).await },
        )
        .await
    }

    /// Find-all-references for the Rust symbol under the cursor, reverse-mapped to `.rsx` `Location`s: refs in this file's generated module map back through the line / expr-span maps; refs in real source files pass through verbatim; refs in *other* generated modules are dropped (a file-scoped `[logic]` symbol has none, and a component's cross-component Rust calls are renamed via the dedicated component-rename path instead). Returns `(locations, unmapped)`: the reverse-mapped reference `Location`s plus a line of generated Rust for each reference that could not be placed (see [`reverse_map_rust_refs`]). Read-only callers ignore `unmapped`; rename refuses when it is non-empty, and says what those lines were.
    pub(crate) async fn rust_reference_locations(
        &self,
        uri: &Uri,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
    ) -> Option<(Vec<Location>, Vec<String>)> {
        let mut refs = self
            .rust_references(rsx_path.clone(), source.clone(), theme.clone(), pos)
            .await?;
        let target = crate::build_sync::generated_target(&rsx_path, &source, theme.as_deref())?;
        refs.extend(
            self.shadow_references(&rsx_path, &source, &target, pos)
                .await,
        );
        Some(reverse_map_rust_refs(
            refs,
            &target.path,
            &target.code,
            &target.map,
            &source,
            uri,
        ))
    }

    /// References to the bindings the transpiler introduced for the symbol under the cursor.
    ///
    /// The clone pass rebinds a captured name so a `move` closure can take it without consuming the original, and rust-analyzer reads each rebinding as its own symbol — correctly, since in the generated code they are. Asking it again at each one is the only way to reach the uses inside those closures, and leaving them out is what made a rename edit the declaration and abandon the `[view]`.
    async fn shadow_references(
        &self,
        rsx_path: &std::path::Path,
        source: &str,
        target: &crate::build_sync::GeneratedTarget,
        pos: Position,
    ) -> Vec<RefTarget> {
        let Some(name) = symbol_at(source, pos) else {
            return Vec::new();
        };
        let offsets: Vec<usize> = target
            .map
            .shadows
            .iter()
            .filter(|shadow| shadow.name == name)
            .map(|shadow| shadow.gen_decl as usize)
            .collect();
        if offsets.is_empty() {
            return Vec::new();
        }
        let Some(root) = crate::build_sync::crate_root(rsx_path) else {
            return Vec::new();
        };
        let gen_path = target.path.clone();
        let gen_code = target.code.clone();
        self.run_analyzer(root, move |analyzer| async move {
            let mut found = Vec::new();
            for offset in offsets {
                found.extend(
                    analyzer
                        .references(&gen_path, &gen_code, offset)
                        .await
                        .unwrap_or_default(),
                );
            }
            Some(found)
        })
        .await
        .unwrap_or_default()
    }
}

/// The identifier under the cursor in the `.rsx`, which is the name a shadow binding would carry.
fn symbol_at(source: &str, pos: Position) -> Option<String> {
    let line = telar_transpiler::nth_line(source, pos.line as usize)?;
    crate::text::ident_at(line, pos.character).map(|(_, ident)| ident.to_owned())
}
/// Where the `.rsx` cursor lands inside the generated module, or `None` when no Rust sits under it.
///
/// `[logic]` is emitted verbatim, so the line map places it and the column only shifts by the body indent. `[view]` has no lines of its own in the output — only the verbatim expressions the transpiler copied — so it resolves through the expression-span map, and a cursor outside every span yields `None`, which is what leaves native element/attribute completion in charge. The offset is a UTF-8 char boundary by construction: the fragment is byte-identical in source and output, and the cursor resolves on a boundary.
pub(crate) fn generated_offset(
    section: Section,
    source: &str,
    generated: &str,
    map: &SourceMap,
    pos: Position,
) -> Option<usize> {
    match section {
        Section::Logic => {
            // First generated line that originated from this `.rsx` line.
            let gen_line = map.lines.iter().position(|m| *m == Some(pos.line))? as u32;
            byte_offset(generated, gen_line, pos.character + LOGIC_INDENT)
        }
        Section::View => {
            let rsx_byte = byte_offset(source, pos.line, pos.character)?;
            // The inclusive upper bound lets the cursor sit right after the last character, the common completion position.
            let span = map.exprs.iter().find(|s| {
                rsx_byte >= s.rsx_start as usize && rsx_byte <= (s.rsx_start + s.len) as usize
            })?;
            Some(span.gen_start as usize + (rsx_byte - span.rsx_start as usize))
        }
        _ => None,
    }
}

/// The offset just inside a component call's props builder, so rust-analyzer answers an attribute key with the setter list — names, types and doc comments, read from the props struct itself.
///
/// An attribute *key* copies no text through, so it has no expression span to map by. What it does have is a generated line: the call this element produced, where `XProps::props()` sits at the head of a chain of one setter per attribute written. Landing right after that first `.` is a method completion on the builder, which is exactly the question the key position is asking.
pub(crate) fn props_builder_offset(
    _section: Section,
    _source: &str,
    generated: &str,
    map: &SourceMap,
    pos: Position,
) -> Option<usize> {
    let mut line_start = 0usize;
    for (index, line) in generated.split_inclusive('\n').enumerate() {
        if map.lines.get(index).copied().flatten() == Some(pos.line)
            && let Some(call) = line.find("Props::props()")
        {
            let after = call + "Props::props().".len();
            return Some(line_start + after);
        }
        line_start += line.len();
    }
    None
}

#[cfg(test)]
#[path = "query_test.rs"]
mod tests;
