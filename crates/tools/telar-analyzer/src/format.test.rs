use super::*;

fn point(line: u32) -> lsp_types::Range {
    lsp_types::Range {
        start: lsp_types::Position { line, character: 0 },
        end: lsp_types::Position { line, character: 0 },
    }
}

#[test]
fn range_edits_only_touch_hunks_in_the_requested_range() {
    let src = "a\nXX\nb\nYY\nc\n";
    let formatted = "a\nxx\nb\nyy\nc\n";
    let edits = range_edits(src, formatted, point(1));
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].range.start.line, 1);
    assert_eq!(edits[0].range.end.line, 2);
    assert_eq!(edits[0].new_text, "xx\n");
}

#[test]
fn range_edits_are_empty_when_the_range_has_no_change() {
    let src = "a\nXX\nb\n";
    let formatted = "a\nxx\nb\n";
    assert!(
        range_edits(src, formatted, point(0)).is_empty(),
        "a range with no change needs no edit"
    );
}

#[test]
fn range_edits_handle_an_insertion_hunk() {
    let src = "@a\n@b\n";
    let formatted = "@a\n\n@b\n";
    let edits = range_edits(src, formatted, point(1));
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].range.start, edits[0].range.end);
    assert_eq!(edits[0].new_text, "\n");
}
