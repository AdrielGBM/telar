use super::*;

#[test]
fn the_lines_after_the_first_lose_their_shared_indentation() {
    let body = "{\n            tally(\n                props,\n            )\n        }";
    assert_eq!(dedent(body), "{\n    tally(\n        props,\n    )\n}");
}

#[test]
fn a_blank_line_does_not_count_towards_the_indentation() {
    let body = "{\n        let a = 1;\n\n        a\n    }";
    assert_eq!(dedent(body), "{\n    let a = 1;\n\n    a\n}");
}

#[test]
fn one_line_is_kept_as_it_is() {
    assert_eq!(dedent("tally(props, children)"), "tally(props, children)");
}

#[test]
fn a_column_counts_characters_from_the_start_of_its_line() {
    let source = "fn a() {}\nlet é = b(1);\n";
    assert_eq!(offset(source, 1, 1), Some(0));
    assert_eq!(offset(source, 2, 6), Some(source.find(" = ").unwrap()));
    assert_eq!(
        offset(source, 2, 14),
        Some(source.len() - 1),
        "the end of the line"
    );
}

#[test]
fn a_position_past_the_source_has_no_offset() {
    let source = "ab\n";
    assert_eq!(offset(source, 3, 2), None);
    assert_eq!(offset(source, 1, 0), None);
    assert_eq!(offset(source, 0, 1), None);
}
