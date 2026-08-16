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

/// Reverse-maps analyzer references onto `.rsx` `Location`s with precise ranges: a real source file passes through verbatim; a reference in *this* file's generated module maps back through the expr-span map (`[view]` verbatim expressions) or the line map (`[logic]` / Props struct); a reference in *another* generated module can't be precisely mapped here. Duplicates are coalesced. Returns `(locations, unmapped)` where `unmapped` counts generated-file references that produced no location — non-zero means the result is incomplete, so a rename must refuse rather than half-apply.
pub(crate) fn reverse_map_rust_refs(
    targets: Vec<RefTarget>,
    current_gen_path: &std::path::Path,
    gen_code: &str,
    map: &SourceMap,
    rsx_source: &str,
    rsx_uri: &Uri,
) -> (Vec<Location>, usize) {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut unmapped = 0usize;
    for target in targets {
        let is_generated = crate::build_sync::is_generated_build_file(&target.path);
        let location = if !is_generated {
            crate::uri::from_path(&target.path).map(|uri| Location {
                uri,
                range: target.range,
            })
        } else if target.path == current_gen_path {
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
            // A generated-file reference we couldn't place (non-verbatim `[view]` fragment, or another component's module) → the reverse-map is lossy here; flag it for the rename guard.
            None if is_generated => unmapped += 1,
            None => {}
        }
    }
    (out, unmapped)
}

/// Reverse-maps a diagnostic's generated-file range onto the `.rsx`, narrowing it to the exact columns when
/// they can be trusted and widening it to the whole line when they cannot.
///
/// The exact mapping was built for go-to-definition and rename, and was never wired here — so every
/// diagnostic underlined its whole line, however precise rustc had been. Which columns can be trusted is
/// [`SourceMap::locate`]'s answer, shared with `cargo telar check` so the terminal and the editor cannot
/// come to two different conclusions about the same error.
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

/// Reverse-maps one generated-file reference span back onto the current `.rsx`. Returns `None` for anything
/// whose columns cannot be trusted — a `[view]` fragment the transpiler rewrote, or a generated line with no
/// `.rsx` origin at all. A diagnostic widens to the line in those cases; a rename must not, because a bogus
/// range here would edit the wrong text.
fn reverse_map_current_file(
    target: &RefTarget,
    gen_code: &str,
    map: &SourceMap,
    rsx_source: &str,
) -> Option<Range> {
    match map.locate(gen_code, target.byte_start, target.byte_end, rsx_source)? {
        RsxSpan::Exact { start, end } => Some(Range {
            start: offset_to_position(rsx_source, start as usize),
            end: offset_to_position(rsx_source, end as usize),
        }),
        RsxSpan::Line(_) => None,
    }
}

/// [`SourceMap::locate`] against an LSP range, which carries UTF-16 columns rather than the byte offsets the
/// map is written in. A range whose start does not convert falls back to its line.
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

/// The `n`-th line of `text` (0-based), without its trailing newline.
/// Width (UTF-16 code units) of the leading space/tab run of `line`. Used by the inlay-hint path, which
/// carries a bare position rather than a range and so cannot go through [`SourceMap::locate`].
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
mod tests {
    use super::*;
    use std::path::PathBuf;

    // url::Url::from_file_path rejects unix-style absolute paths on Windows, so tests build platform-valid ones.
    fn abs(unix: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!("C:{}", unix.replace('/', "\\")))
        } else {
            PathBuf::from(unix)
        }
    }

    fn target(
        gen_line: u32,
        gen_char_start: u32,
        gen_char_end: u32,
        bytes: (u32, u32),
    ) -> RefTarget {
        RefTarget {
            path: PathBuf::from("/x/.telar/build/c.rs"),
            byte_start: bytes.0,
            byte_end: bytes.1,
            range: Range {
                start: Position {
                    line: gen_line,
                    character: gen_char_start,
                },
                end: Position {
                    line: gen_line,
                    character: gen_char_end,
                },
            },
        }
    }

    #[test]
    fn logic_ref_maps_back_subtracting_the_indent() {
        // Generated `[logic]` lines carry a +4 indent; the line map ties gen line 1 to .rsx line 1.
        let gen_src = "boilerplate\n    let total = x;\n";
        let rsx = "[logic]\nlet total = x;\n";
        let map = SourceMap::new(vec![None, Some(1)], vec![]);
        // `total` sits at gen col 8 (`    let `); expect rsx col 4 (`let `).
        let at = gen_src.find("total").unwrap() as u32;
        let t = target(1, 8, 13, (at, at + 5));
        let range = reverse_map_current_file(&t, gen_src, &map, rsx).unwrap();
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 4);
        assert_eq!(range.end.character, 9);
    }

    #[test]
    fn view_ref_maps_through_the_expr_span() {
        // A verbatim `[view]` expression: the `name` fragment is byte-identical in source and gen.
        let rsx = "[view]\ncol\n    text \"{name}\"\n";
        let rsx_name_byte = rsx.find("name").unwrap() as u32;
        let gen_src = "fn c() {\n    text(format!(\"{}\", name))\n}\n";
        let gen_name_byte = gen_src.find("name").unwrap() as u32;
        let map = SourceMap::new(
            vec![],
            vec![telar_transpiler::ExprSpan {
                rsx_start: rsx_name_byte,
                len: 4,
                gen_start: gen_name_byte,
            }],
        );
        let t = target(1, 0, 0, (gen_name_byte, gen_name_byte + 4));
        let range = reverse_map_current_file(&t, gen_src, &map, rsx).unwrap();
        // `name` is on .rsx line 2 (`    text "{name}"`), at the `{`+1 column.
        assert_eq!(range.start.line, 2);
        let col = "    text \"{".encode_utf16().count() as u32;
        assert_eq!(range.start.character, col);
        assert_eq!(range.end.character, col + 4);
    }

    #[test]
    fn boilerplate_lines_have_no_origin() {
        let gen_src = "boilerplate\n";
        let map = SourceMap::new(vec![None], vec![]);
        let t = target(0, 0, 3, (0, 0));
        assert!(reverse_map_current_file(&t, gen_src, &map, "x\n").is_none());
    }

    #[test]
    fn real_files_pass_through_generated_files_reverse_map() {
        let uri: Uri = "file:///x/src/c.rsx".parse().unwrap();
        let real = RefTarget {
            path: abs("/x/src/lib.rs"),
            byte_start: 0,
            byte_end: 4,
            range: Range {
                start: Position {
                    line: 5,
                    character: 2,
                },
                end: Position {
                    line: 5,
                    character: 6,
                },
            },
        };
        let (locs, unmapped) = reverse_map_rust_refs(
            vec![real],
            &abs("/x/.telar/build/c.rs"),
            "",
            &SourceMap::default(),
            "",
            &uri,
        );
        assert_eq!(locs.len(), 1);
        assert!(locs[0].uri.as_str().ends_with("lib.rs"));
        assert_eq!(locs[0].range.start.line, 5);
        assert_eq!(unmapped, 0);
    }

    #[test]
    fn view_ref_without_a_span_is_dropped_not_corrupted() {
        // A `[view]` reference outside any verbatim expr-span (e.g. an `img src:foo` attr value) must be dropped and counted as unmapped — never mapped with a bogus column (which corrupted renames).
        let uri: Uri = "file:///x/src/c.rsx".parse().unwrap();
        let gen_path = abs("/x/.telar/build/c.rs");
        let rsx = "[view]\ncol\n    img src:foo\n";
        let gen_src = "fn c() {\n    let __src = foo.clone();\n}\n";
        // gen line 1 → rsx line 2 (`    img src:foo`, a `[view]` line).
        let map = SourceMap::new(vec![None, Some(2), None], vec![]);
        let t = RefTarget {
            path: gen_path.to_path_buf(),
            byte_start: 0,
            byte_end: 0,
            range: Range {
                start: Position {
                    line: 1,
                    character: 15,
                },
                end: Position {
                    line: 1,
                    character: 18,
                },
            },
        };
        let (locs, unmapped) = reverse_map_rust_refs(vec![t], &gen_path, gen_src, &map, rsx, &uri);
        assert!(locs.is_empty());
        assert_eq!(unmapped, 1);
    }

    /// A diagnostic in `[logic]` lands on the columns rustc named, shifted by the indent the transpiler adds.
    /// Before this, every diagnostic underlined its whole line however precise rustc had been — the exact
    /// mapping existed, but only go-to-definition and rename ever used it.
    #[test]
    fn a_logic_diagnostic_keeps_the_columns_rustc_gave_it() {
        let rsx = "[logic]\nlet count = signal(0);\n\n[view]\ncolumn\n";
        // The generated line is the same text under four spaces of indent.
        let generated = "fn demo() {\n    let count = signal(0);\n}\n";
        let map = SourceMap::new(vec![None, Some(1), None], vec![]);
        let range = Range {
            start: Position {
                line: 1,
                character: 8,
            },
            end: Position {
                line: 1,
                character: 13,
            },
        };

        let mapped = diagnostic_range(range, generated, &map, rsx).expect("the line maps");
        assert_eq!(mapped.start.line, 1);
        assert_eq!(
            (mapped.start.character, mapped.end.character),
            (4, 9),
            "the four spaces the transpiler added come back off"
        );
    }

    /// And the case that has to stay wide: a `[view]` line the transpiler rewrote has no column
    /// correspondence at all, so a narrowed range would underline text that has nothing to do with the error.
    #[test]
    fn a_view_diagnostic_still_takes_the_whole_line() {
        let rsx = "[logic]\nlet x = 1;\n\n[view]\ntext \"hi\"\n";
        let generated = "fn demo() {\n    Text::new(\"hi\")\n}\n";
        let map = SourceMap::new(vec![None, Some(4), None], vec![]);
        let range = Range {
            start: Position {
                line: 1,
                character: 4,
            },
            end: Position {
                line: 1,
                character: 8,
            },
        };

        let mapped = diagnostic_range(range, generated, &map, rsx).expect("the line maps");
        assert_eq!(
            (mapped.start.character, mapped.end.character),
            (0, u32::MAX)
        );
    }
}
