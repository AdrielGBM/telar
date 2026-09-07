use super::*;

/// The half that was already persisted, unchanged: a `[logic]` line is transpiled 1:1 under a fixed indent, so rustc's columns come back by subtracting it.
#[test]
fn a_logic_span_comes_back_minus_the_indent_the_transpiler_added() {
    let rsx = "[logic]\nlet count = signal(0);\n\n[view]\ncolumn\n";
    let generated = "fn demo() {\n    let count = signal(0);\n}\n";
    let map = SourceMap::new(vec![None, Some(1), None], vec![]);

    let at = generated.find("count").unwrap() as u32;
    let span = map.locate(generated, at, at + 5, rsx).unwrap();
    let start = rsx.find("count").unwrap() as u32;
    assert_eq!(
        span,
        RsxSpan::Exact {
            start,
            end: start + 5
        }
    );
}

/// The half that was not, and the reason this module exists: a verbatim `[view]` expression is the same bytes in both files, so rustc's columns land on it exactly.
#[test]
fn a_verbatim_view_expression_maps_byte_for_byte() {
    let rsx = "[view]\ncol\n    text \"{name}\"\n";
    let generated = "fn c() {\n    text(format!(\"{}\", name))\n}\n";
    let rsx_start = rsx.find("name").unwrap() as u32;
    let gen_start = generated.find("name").unwrap() as u32;
    let map = SourceMap::new(
        vec![],
        vec![ExprSpan {
            rsx_start,
            len: 4,
            gen_start,
        }],
    );

    assert_eq!(
        map.locate(generated, gen_start, gen_start + 4, rsx),
        Some(RsxSpan::Exact {
            start: rsx_start,
            end: rsx_start + 4
        })
    );
}

/// And the case that has to stay wide. A `[view]` line the transpiler rewrote has no column correspondence at all, so the honest answer is the line.
#[test]
fn a_rewritten_view_line_widens_to_the_whole_line() {
    let rsx = "[logic]\nlet x = 1;\n\n[view]\ntext \"hi\"\n";
    let generated = "fn demo() {\n    Text::new(\"hi\")\n}\n";
    let map = SourceMap::new(vec![None, Some(4), None], vec![]);

    let at = generated.find("Text").unwrap() as u32;
    assert_eq!(
        map.locate(generated, at, at + 4, rsx),
        Some(RsxSpan::Line(4))
    );
}

#[test]
fn a_boilerplate_line_maps_to_nothing() {
    let map = SourceMap::new(vec![None], vec![]);
    assert_eq!(map.locate("boilerplate\n", 0, 3, "x\n"), None);
}

/// The sidecar has to carry both halves or the CLI is back to underlining lines.
#[test]
fn both_halves_survive_the_round_trip_through_json() {
    let map = SourceMap::new(
        vec![None, Some(3)],
        vec![ExprSpan {
            rsx_start: 7,
            len: 4,
            gen_start: 21,
        }],
    );
    let back = SourceMap::from_json(&map.to_json()).unwrap();
    assert_eq!(back.lines, vec![None, Some(3)]);
    assert_eq!(back.exprs.len(), 1);
    assert_eq!(
        (
            back.exprs[0].rsx_start,
            back.exprs[0].len,
            back.exprs[0].gen_start
        ),
        (7, 4, 21)
    );
}

/// A multibyte line ahead of the span: the offsets are bytes throughout, so nothing has to agree about what a column is.
#[test]
fn offsets_stay_right_after_a_multibyte_line() {
    let rsx = "[logic]\nlet título = 1;\nlet n = 2;\n";
    let generated = "fn c() {\n    let título = 1;\n    let n = 2;\n}\n";
    let map = SourceMap::new(vec![None, Some(1), Some(2), None], vec![]);

    let at = generated.find("n = 2").unwrap() as u32;
    let start = rsx.find("n = 2").unwrap() as u32;
    assert_eq!(
        map.locate(generated, at, at + 1, rsx),
        Some(RsxSpan::Exact {
            start,
            end: start + 1
        })
    );
}
