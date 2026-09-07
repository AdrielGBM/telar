use super::*;

const SRC: &str = "[logic]\nlet count = signal(0i32);\n[view]\ncol @card\n    text \"Hi\"\n    row gap:8\n        btn \"+\"\n";

/// Collects a hierarchy outermost→innermost as `(start_line, start_col)-(end_line, end_col)`.
fn chain(node: &SelectionRange) -> Vec<((u32, u32), (u32, u32))> {
    let mut out = Vec::new();
    let mut cur = Some(node);
    while let Some(n) = cur {
        out.push((
            (n.range.start.line, n.range.start.character),
            (n.range.end.line, n.range.end.character),
        ));
        cur = n.parent.as_deref();
    }
    out.reverse();
    out
}

#[test]
fn expands_word_to_line_to_block_to_section_to_document() {
    let lines: Vec<&str> = SRC.lines().collect();
    let node = selection_for(
        &lines,
        Position {
            line: 6,
            character: 9,
        },
    );
    let c = chain(&node);
    assert_eq!(c.first().unwrap().0, (0, 0));
    let inner = c.last().unwrap();
    assert_eq!(inner.0, (6, 8));
    assert_eq!(inner.1, (6, 11));
    assert!(c.iter().any(|&(s, _)| s == (2, 0)), "section level: {c:?}");
    for w in c.windows(2) {
        let (outer, inner) = (w[0], w[1]);
        assert!(
            outer.0 <= inner.0 && inner.1 <= outer.1,
            "not nested: {c:?}"
        );
    }
}

const WITH_PREVIEW: &str =
    "[view]\ncol\n    text \"Hi\"\n[preview \"Default\"]\ncounter\n    text \"x\"\n";

#[test]
fn the_view_section_stops_at_a_preview_header() {
    let lines: Vec<&str> = WITH_PREVIEW.lines().collect();
    let c = chain(&selection_for(
        &lines,
        Position {
            line: 1,
            character: 0,
        },
    ));
    assert!(
        c.iter().any(|&(start, end)| start == (0, 0) && end.0 == 2),
        "expected a [view] section ending at line 2: {c:?}"
    );
    assert!(
        !c.iter()
            .any(|&(start, end)| start.0 == 0 && (end.0 == 3 || end.0 == 4)),
        "a level swept part of the preview into the view selection: {c:?}"
    );
}

#[test]
fn a_preview_is_its_own_section() {
    let lines: Vec<&str> = WITH_PREVIEW.lines().collect();
    let c = chain(&selection_for(
        &lines,
        Position {
            line: 5,
            character: 4,
        },
    ));
    assert!(
        c.iter().any(|&(start, end)| start == (3, 0) && end.0 == 5),
        "expected a [preview] section spanning lines 3..5: {c:?}"
    );
    assert!(
        !c.iter().any(|&(start, _)| start.0 < 3 && start != (0, 0)),
        "no level should reach back into the [view] section: {c:?}"
    );
}

#[test]
fn block_groups_a_parent_with_its_children() {
    let lines: Vec<&str> = SRC.lines().collect();
    let (start, end) = block_bounds(&lines, 5);
    assert_eq!((start, end), (5, 6));
}
