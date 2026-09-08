//! Mapping a position in the generated Rust back onto the `.rsx` line and column it came from.

use lsp_types::*;

use crate::ra::{DefinitionTarget, RefTarget};
use crate::text::offset_to_position;
use telar_transpiler::{RsxSpan, SourceMap};

/// Maps rust-analyzer definition targets to `.rsx` `Location`s, handling three cases per target: (1) the generated `.rs` for *this* `.rsx` and (2) *another* component's `.telar/build/*.rs` are both reverse-mapped through that build file's line map (`generated line → .rsx line`) onto its `.rsx` source — a generated line with no originating `.rsx` line is dropped; (3) any other path (a dependency, std, or a hand-written `.rs`) is returned verbatim in its own coordinates.
pub(crate) fn map_definition_targets(targets: Vec<DefinitionTarget>) -> Vec<Location> {
    let mut locations = Vec::new();
    for target in targets {
        if crate::build_sync::is_generated_build_file(&target.path) {
            // Cases 1 & 2: a generated build file → walk back to its `.rsx` via the sibling `.rs.map`.
            let Some((rsx_path, map)) = crate::build_sync::rsx_source_and_map(&target.path) else {
                continue;
            };
            let Some(Some(rsx_line)) = map.lines.get(target.range.start.line as usize) else {
                continue;
            };
            if let Some(uri) = crate::uri::from_path(&rsx_path) {
                locations.push(Location {
                    uri,
                    range: Range {
                        start: Position {
                            line: *rsx_line,
                            character: 0,
                        },
                        end: Position {
                            line: *rsx_line,
                            character: 0,
                        },
                    },
                });
            }
        } else if let Some(uri) = crate::uri::from_path(&target.path) {
            // Case 3: a real source file → jump straight to its own range.
            locations.push(Location {
                uri,
                range: target.range,
            });
        }
    }
    locations
}

/// Whether `target` sits on either half of a `let x = x.clone();` the transpiler wrote — the name it declares, or the name it reads to initialise it. Both are generated text with no `.rsx` counterpart, and both come back with the new spelling when the file is transpiled again, so a rename has nothing to do about them and no reason to refuse over them.
fn lands_on_a_shadow(target: &RefTarget, gen_code: &str, map: &SourceMap) -> bool {
    let Some(at) = crate::text::byte_offset(
        gen_code,
        target.range.start.line,
        target.range.start.character,
    ) else {
        return false;
    };
    map.shadows.iter().any(|shadow| {
        let declared = shadow.gen_decl as usize;
        // The statement is emitted as `let {name} = {name}.clone();`, so the initialiser's name follows the declared one by the ` = ` between them.
        let initialiser = declared + shadow.name.len() + 3;
        at == declared || at == initialiser
    })
}
/// Reverse-maps analyzer references onto `.rsx` `Location`s with precise ranges: a real source file passes through verbatim; a reference in *this* file's generated module maps back through the expr-span map (`[view]` verbatim expressions) or the line map (`[logic]` / Props struct); a reference in *another* generated module can't be precisely mapped here. Duplicates are coalesced. Returns `(locations, unmapped)` where `unmapped` counts generated-file references that produced no location — non-zero means the result is incomplete, so a rename must refuse rather than half-apply.
pub(crate) fn reverse_map_rust_refs(
    targets: Vec<RefTarget>,
    current_gen_path: &std::path::Path,
    gen_code: &str,
    map: &SourceMap,
    rsx_source: &str,
    rsx_uri: &Uri,
) -> (Vec<Location>, Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut unmapped: Vec<String> = Vec::new();
    for target in targets {
        let is_generated = crate::build_sync::is_generated_build_file(&target.path);
        let location = if !is_generated {
            crate::uri::from_path(&target.path).map(|uri| Location {
                uri,
                range: target.range,
            })
        } else if target.path == current_gen_path {
            // A reference to the shadow's own `let` is not a use anywhere: the transpiler writes that line, and regenerating the file after the rename writes it again under the new name. Counting it as unmapped is what refused a rename that had nothing left to place.
            if lands_on_a_shadow(&target, gen_code, map) {
                continue;
            }
            reverse_map_current_file(&target, gen_code, map, rsx_source).map(|range| Location {
                uri: rsx_uri.clone(),
                range,
            })
        } else {
            None
        };
        match location {
            Some(location) => {
                let key = (
                    location.uri.as_str().to_string(),
                    location.range.start.line,
                    location.range.start.character,
                    location.range.end.character,
                );
                if seen.insert(key) {
                    out.push(location);
                }
            }
            // A generated-file reference we couldn't place (non-verbatim `[view]` fragment, or another component's module). Reported as the generated line itself rather than a count: what the line says is what tells anyone reading the refusal which construction defeated the map.
            None if is_generated => {
                let at = target.range.start.line;
                let text = telar_transpiler::nth_line(gen_code, at as usize)
                    .unwrap_or("")
                    .trim();
                unmapped.push(format!("generated line {at}: {text}"));
            }
            None => {}
        }
    }
    (out, unmapped)
}

/// Reverse-maps a diagnostic's generated-file range onto the `.rsx`, narrowing it to the exact columns when they can be trusted and widening it to the whole line when they cannot.
///
/// The exact mapping was built for go-to-definition and rename, and was never wired here — so every diagnostic underlined its whole line, however precise rustc had been. Which columns can be trusted is [`SourceMap::locate`]'s answer, shared with `cargo telar check` so the terminal and the editor cannot come to two different conclusions about the same error.
pub(crate) fn diagnostic_range(
    gen_range: Range,
    gen_code: &str,
    map: &SourceMap,
    rsx_source: &str,
) -> Option<Range> {
    match locate(map, gen_range, gen_code, rsx_source)? {
        RsxSpan::Exact { start, end } => Some(Range {
            start: offset_to_position(rsx_source, start as usize),
            end: offset_to_position(rsx_source, end as usize),
        }),
        RsxSpan::Line(line) => Some(Range {
            start: Position { line, character: 0 },
            end: Position {
                line,
                character: u32::MAX,
            },
        }),
    }
}

/// Reverse-maps one generated-file reference span back onto the current `.rsx`. Returns `None` for anything whose columns cannot be trusted — a `[view]` fragment the transpiler rewrote, or a generated line with no `.rsx` origin at all. A diagnostic widens to the line in those cases; a rename must not, because a bogus range here would edit the wrong text.
fn reverse_map_current_file(
    target: &RefTarget,
    gen_code: &str,
    map: &SourceMap,
    rsx_source: &str,
) -> Option<Range> {
    match locate(map, target.range, gen_code, rsx_source)? {
        RsxSpan::Exact { start, end } => Some(Range {
            start: offset_to_position(rsx_source, start as usize),
            end: offset_to_position(rsx_source, end as usize),
        }),
        RsxSpan::Line(_) => None,
    }
}

/// [`SourceMap::locate`] against an LSP range, which carries UTF-16 columns rather than the byte offsets the map is written in. A range whose start does not convert falls back to its line.
fn locate(map: &SourceMap, gen_range: Range, gen_code: &str, rsx_source: &str) -> Option<RsxSpan> {
    let Some(byte_start) =
        crate::text::byte_offset(gen_code, gen_range.start.line, gen_range.start.character)
    else {
        return map
            .lines
            .get(gen_range.start.line as usize)?
            .map(RsxSpan::Line);
    };
    let byte_end = crate::text::byte_offset(gen_code, gen_range.end.line, gen_range.end.character)
        .unwrap_or(byte_start);
    map.locate(gen_code, byte_start as u32, byte_end as u32, rsx_source)
}

/// The `n`-th line of `text` (0-based), without its trailing newline. Width (UTF-16 code units) of the leading space/tab run of `line`. Used by the inlay-hint path, which carries a bare position rather than a range and so cannot go through [`SourceMap::locate`].
pub(crate) fn leading_ws_utf16(line: &str) -> u32 {
    line.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| c.len_utf16() as u32)
        .sum()
}

/// Builds the range covering all of `source`, used to replace the whole document with its formatted form. Character offsets are UTF-16 code units, per LSP.
pub(crate) fn full_document_range(source: &str) -> Range {
    let mut line = 0u32;
    let mut last_line_len = 0u32;
    for chunk in source.split_inclusive('\n') {
        if chunk.ends_with('\n') {
            line += 1;
            last_line_len = 0;
        } else {
            last_line_len = chunk.encode_utf16().count() as u32;
        }
    }
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line,
            character: last_line_len,
        },
    }
}

#[cfg(test)]
#[path = "mapping_test.rs"]
mod tests;
