//! Asking the embedded analyzer about a position, with the `.rsx` cursor mapped into the generated Rust.

use std::path::PathBuf;

use lsp_types::*;
use ra_ap_ide::TextSize;
use telar_transpiler::SourceMap;

use crate::position::{Section, find_section_at};
use crate::ra::{EmbeddedAnalyzer, RefTarget};
use crate::text::byte_offset;

use super::mapping::reverse_map_rust_refs;
use super::{AnalyzerState, Backend, mark_reload};

/// `[logic]` lines are emitted verbatim under a fixed function-body indent, so an `.rsx` column maps to the generated column by adding this.
const LOGIC_INDENT: u32 = 4;

impl Backend {
    /// Maps an `.rsx` cursor into the generated module and runs `run` against the embedded analyzer on a blocking thread (the query is synchronous; the load may still be in flight, in which case this yields `None`). One entry point for both sections, since only the cursor resolution differs — see [`generated_offset`].
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
        self.rust_query_at(rsx_path, source, theme, pos, generated_offset, run)
            .await
    }

    /// The same, with the cursor resolved by `locate` instead of by section. One entry point, because only the resolution differs — an attribute *key* has no expression span of its own, so it maps to the props builder that carries its setter rather than to itself.
    pub(crate) async fn rust_query_at<T, F, L>(
        &self,
        rsx_path: PathBuf,
        source: String,
        theme: Option<String>,
        pos: Position,
        locate: L,
        run: F,
    ) -> Option<T>
    where
        F: FnOnce(&mut EmbeddedAnalyzer, PathBuf, String, TextSize) -> Option<T> + Send + 'static,
        L: FnOnce(Section, &str, &str, &SourceMap, Position) -> Option<TextSize>,
        T: Send + 'static,
    {
        let section = find_section_at(&source, pos.line);
        let crate::build_sync::GeneratedTarget {
            path: gen_path,
            code: gen_text,
            map,
        } = crate::build_sync::generated_target(&rsx_path, &source, theme.as_deref())?;
        let offset = locate(section, &source, &gen_text, &map, pos)?;

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
            // A generated module the graph does not know yet, so drop to `Idle` and let the next query reload.
            if !a.knows_file(&gen_path) {
                mark_reload(&reload_at);
                return None;
            }
            let ra_at = std::time::Instant::now();
            let result = run(a, gen_path, gen_text, offset);
            // A healthy query is sub-100ms, so anything past a second flags a regression without spamming the output.
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
/// `[logic]` is emitted verbatim, so the line map places it and the column only shifts by the body indent. `[view]` has no lines of its own in the output — only the verbatim expressions the transpiler copied — so it resolves through the expression-span map, and a cursor outside every span yields `None`, which is what leaves native element/attribute completion in charge. The offset is a UTF-8 char boundary by construction: the fragment is byte-identical in source and output, and the cursor resolves on a boundary.
pub(crate) fn generated_offset(
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
            // The inclusive upper bound lets the cursor sit right after the last character, the common completion position.
            let span = map.exprs.iter().find(|s| {
                rsx_byte >= s.rsx_start as usize && rsx_byte <= (s.rsx_start + s.len) as usize
            })?;
            span.gen_start as usize + (rsx_byte - span.rsx_start as usize)
        }
        _ => return None,
    };
    Some(TextSize::from(byte as u32))
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
) -> Option<TextSize> {
    let mut line_start = 0usize;
    for (index, line) in generated.split_inclusive('\n').enumerate() {
        if map.lines.get(index).copied().flatten() == Some(pos.line)
            && let Some(call) = line.find("Props::props()")
        {
            let after = call + "Props::props().".len();
            return Some(TextSize::from((line_start + after) as u32));
        }
        line_start += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use telar_transpiler::transpile_source;

    /// The key position lands on the builder, so what comes back is the props struct's own setters rather than a table this crate would have to keep in step with every component in the workspace.
    #[test]
    fn an_attribute_key_maps_to_the_props_builder_that_carries_its_setter() {
        let rsx =
            "[logic]\nuse crate::ui::card::{CardProps, card};\n\n[view]\ncol\n    card pad:8\n";
        let out = transpile_source(rsx, "demo", None, None).unwrap();
        let map = SourceMap::new(out.source_map.clone(), out.expr_spans.clone());

        // `card pad:8` is the sixth line of the `.rsx`, zero-based 5.
        let offset = props_builder_offset(
            Section::View,
            rsx,
            &out.rust_code,
            &map,
            Position::new(5, 9),
        )
        .expect("the element's generated line carries a props builder");

        let at = usize::from(offset);
        assert_eq!(
            &out.rust_code[at - "CardProps::props().".len()..at],
            "CardProps::props().",
            "the cursor sits where a method completion answers with the setters"
        );
    }

    /// A built-in tag never reaches here, and a line that produced no component call has no builder to point at — saying so is what keeps the caller on its own answer instead of a wrong offset.
    #[test]
    fn a_line_with_no_component_call_maps_nowhere() {
        let rsx = "[view]\ncol gap:8\n    text \"x\"\n";
        let out = transpile_source(rsx, "demo", None, None).unwrap();
        let map = SourceMap::new(out.source_map.clone(), out.expr_spans.clone());
        assert!(
            props_builder_offset(
                Section::View,
                rsx,
                &out.rust_code,
                &map,
                Position::new(1, 4)
            )
            .is_none()
        );
    }
}
