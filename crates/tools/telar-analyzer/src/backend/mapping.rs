use lsp_types::*;

use crate::position::{Section, find_section_at};
use crate::ra::{DefinitionTarget, RefTarget};
use crate::text::offset_to_position;
use telar_transpiler::ExprSpan;

/// Maps rust-analyzer definition targets to `.rsx` `Location`s, handling three cases per target: (1) the generated `.rs` for *this* `.rsx` and (2) *another* component's `.telar/build/*.rs` are both reverse-mapped through that build file's line map (`generated line → .rsx line`) onto its `.rsx` source — a generated line with no originating `.rsx` line is dropped; (3) any other path (a dependency, std, or a hand-written `.rs`) is returned verbatim in its own coordinates.
pub(crate) fn map_definition_targets(targets: Vec<DefinitionTarget>) -> Vec<Location> {
    let mut locations = Vec::new();
    for target in targets {
        if crate::build_sync::is_generated_build_file(&target.path) {
            // Cases 1 & 2: a generated build file → walk back to its `.rsx` via the sibling `.rs.map`.
            let Some((rsx_path, map)) = crate::build_sync::rsx_source_and_map(&target.path) else {
                continue;
            };
            let Some(Some(rsx_line)) = map.get(target.range.start.line as usize) else {
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
    map: &[Option<u32>],
    expr_spans: &[ExprSpan],
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
            reverse_map_current_file(&target, gen_code, map, expr_spans, rsx_source).map(|range| {
                Location {
                    uri: rsx_uri.clone(),
                    range,
                }
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
/// diagnostic underlined its whole line, however precise rustc had been. The distinction the reference path
/// already draws is the one that matters: a `[view]` verbatim expression maps byte for byte through its
/// `ExprSpan`, and `[logic]` maps by line plus the indent delta because it is transpiled 1:1. Anything else
/// in `[view]` — an attribute value the transpiler rewrote into something else entirely — has no column
/// correspondence, and a narrow range there would underline the wrong text, which is worse than the line.
pub(crate) fn diagnostic_range(
    gen_range: Range,
    gen_code: &str,
    map: &[Option<u32>],
    expr_spans: &[ExprSpan],
    rsx_source: &str,
) -> Option<Range> {
    let rsx_line = (*map.get(gen_range.start.line as usize)?)?;
    let whole_line = Range {
        start: Position {
            line: rsx_line,
            character: 0,
        },
        end: Position {
            line: rsx_line,
            character: u32::MAX,
        },
    };
    let Some(byte_start) =
        crate::text::byte_offset(gen_code, gen_range.start.line, gen_range.start.character)
    else {
        return Some(whole_line);
    };
    let byte_end = crate::text::byte_offset(gen_code, gen_range.end.line, gen_range.end.character)
        .unwrap_or(byte_start);
    let target = crate::ra::RefTarget {
        path: std::path::PathBuf::new(),
        byte_start: byte_start as u32,
        byte_end: byte_end as u32,
        range: gen_range,
    };
    Some(
        reverse_map_current_file(&target, gen_code, map, expr_spans, rsx_source)
            .unwrap_or(whole_line),
    )
}

/// Reverse-maps one generated-file reference span back onto the current `.rsx`. `[view]` verbatim expressions map byte-for-byte through their `ExprSpan`; everything else is line-mapped, with the column shifted by the leading-whitespace delta between the generated and `.rsx` lines (`+4` for `[logic]`, `0` for the verbatim Props struct). Returns `None` when the generated line has no `.rsx` origin (boilerplate / transpiler-injected).
fn reverse_map_current_file(
    target: &RefTarget,
    gen_code: &str,
    map: &[Option<u32>],
    expr_spans: &[ExprSpan],
    rsx_source: &str,
) -> Option<Range> {
    if let Some(span) = expr_spans
        .iter()
        .find(|s| target.byte_start >= s.gen_start && target.byte_start < s.gen_start + s.len)
    {
        let span_end = span.gen_start + span.len;
        let rsx_start = span.rsx_start + (target.byte_start - span.gen_start);
        let rsx_end = span.rsx_start + (target.byte_end.min(span_end) - span.gen_start);
        return Some(Range {
            start: offset_to_position(rsx_source, rsx_start as usize),
            end: offset_to_position(rsx_source, rsx_end as usize),
        });
    }
    let gen_line = target.range.start.line as usize;
    let rsx_line = (*map.get(gen_line)?)?;
    // The line-map + indent-delta column math only holds for `[logic]` (lines emitted verbatim under a fixed indent, incl. the Props struct). A `[view]`/`[preview]` reference that fell through the expr-span check above (e.g. an `img src:foo` attr value) has no column correspondence — drop it rather than emit a bogus range that would mis-highlight and corrupt a rename.
    if find_section_at(rsx_source, rsx_line) != Section::Logic {
        return None;
    }
    let gen_line_text = nth_line(gen_code, gen_line)?;
    let rsx_line_text = nth_line(rsx_source, rsx_line as usize).unwrap_or("");
    let delta = leading_ws_utf16(gen_line_text).saturating_sub(leading_ws_utf16(rsx_line_text));
    Some(Range {
        start: Position {
            line: rsx_line,
            character: target.range.start.character.saturating_sub(delta),
        },
        end: Position {
            line: rsx_line,
            character: target.range.end.character.saturating_sub(delta),
        },
    })
}

/// The `n`-th line of `text` (0-based), without its trailing newline.
pub(crate) fn nth_line(text: &str, n: usize) -> Option<&str> {
    text.split_inclusive('\n')
        .nth(n)
        .map(|line| line.strip_suffix('\n').unwrap_or(line))
}

/// Width (UTF-16 code units) of the leading space/tab run of `line`.
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
        let map = vec![None, Some(1)];
        // `total` sits at gen col 8 (`    let `); expect rsx col 4 (`let `).
        let t = target(1, 8, 13, (0, 0));
        let range = reverse_map_current_file(&t, gen_src, &map, &[], rsx).unwrap();
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
        let spans = vec![ExprSpan {
            rsx_start: rsx_name_byte,
            len: 4,
            gen_start: gen_name_byte,
        }];
        let t = target(1, 0, 0, (gen_name_byte, gen_name_byte + 4));
        let range = reverse_map_current_file(&t, gen_src, &[], &spans, rsx).unwrap();
        // `name` is on .rsx line 2 (`    text "{name}"`), at the `{`+1 column.
        assert_eq!(range.start.line, 2);
        let col = "    text \"{".encode_utf16().count() as u32;
        assert_eq!(range.start.character, col);
        assert_eq!(range.end.character, col + 4);
    }

    #[test]
    fn boilerplate_lines_have_no_origin() {
        let gen_src = "boilerplate\n";
        let map = vec![None];
        let t = target(0, 0, 3, (0, 0));
        assert!(reverse_map_current_file(&t, gen_src, &map, &[], "x\n").is_none());
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
            &[],
            &[],
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
        let map = vec![None, Some(2), None];
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
        let (locs, unmapped) =
            reverse_map_rust_refs(vec![t], &gen_path, gen_src, &map, &[], rsx, &uri);
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
        let map = vec![None, Some(1), None];
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

        let mapped = diagnostic_range(range, generated, &map, &[], rsx).expect("the line maps");
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
        let map = vec![None, Some(4), None];
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

        let mapped = diagnostic_range(range, generated, &map, &[], rsx).expect("the line maps");
        assert_eq!(
            (mapped.start.character, mapped.end.character),
            (0, u32::MAX)
        );
    }
}
