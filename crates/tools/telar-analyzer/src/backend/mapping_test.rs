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

fn target(gen_line: u32, gen_char_start: u32, gen_char_end: u32) -> RefTarget {
    RefTarget {
        path: PathBuf::from("/x/.telar/build/c.rs"),
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
    let t = target(1, 8, 13);
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
    // The span rust-analyzer reports for that reference, on generated line 1.
    let gen_col = gen_src.lines().nth(1).unwrap().find("name").unwrap() as u32;
    let t = target(1, gen_col, gen_col + 4);
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
    let t = target(0, 0, 3);
    assert!(
        reverse_map_current_file(&t, gen_src, &map, "x\n").is_none(),
        "a boilerplate line maps back to no .rsx text"
    );
}

#[test]
fn real_files_pass_through_generated_files_reverse_map() {
    let uri: Uri = "file:///x/src/c.rsx".parse().unwrap();
    let real = RefTarget {
        path: abs("/x/src/lib.rs"),
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
    assert!(
        locs[0].uri.as_str().ends_with("lib.rs"),
        "a real file maps to itself: {}",
        locs[0].uri.as_str()
    );
    assert_eq!(locs[0].range.start.line, 5);
    assert!(
        unmapped.is_empty(),
        "nothing should have gone unplaced: {unmapped:?}"
    );
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
    assert!(
        locs.is_empty(),
        "a view ref with no span is dropped rather than pointing somewhere wrong"
    );
    assert_eq!(
        unmapped.len(),
        1,
        "expected one unplaced reference: {unmapped:?}"
    );
}

/// A diagnostic in `[logic]` lands on the columns rustc named, shifted by the indent the transpiler adds. Before this, every diagnostic underlined its whole line however precise rustc had been — the exact mapping existed, but only go-to-definition and rename ever used it.
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

/// And the case that has to stay wide: a `[view]` line the transpiler rewrote has no column correspondence at all, so a narrowed range would underline text that has nothing to do with the error.
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
