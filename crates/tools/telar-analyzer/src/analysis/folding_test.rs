use super::*;

fn has(ranges: &[FoldingRange], start: u32, end: u32) -> bool {
    ranges
        .iter()
        .any(|r| r.start_line == start && r.end_line == end)
}

#[test]
fn folds_sections_and_nested_blocks() {
    let src = "[logic]\nlet x = 1;\n[style]\n@card\n    width: 240\n    gap: 8\n[view]\ncol\n    text \"a\"\n    text \"b\"\n";
    let folds = folding_ranges(src);
    assert!(has(&folds, 0, 1), "[logic] section:\n{folds:?}");
    assert!(has(&folds, 2, 5), "[style] section:\n{folds:?}");
    assert!(has(&folds, 6, 9), "[view] section:\n{folds:?}");
    assert!(has(&folds, 3, 5), "@card block:\n{folds:?}");
    assert!(has(&folds, 7, 9), "col block:\n{folds:?}");
}

#[test]
fn preview_section_is_foldable() {
    let src = "[view]\ncol\n\n[preview \"Tall\"]\nbox\n    text \"hi\"\n";
    let folds = folding_ranges(src);
    assert!(has(&folds, 3, 5), "[preview] section:\n{folds:?}");
}

#[test]
fn the_previews_meta_section_and_a_play_zone_fold_on_their_own() {
    let src = "[view]\ncol\n    text \"x\"\n[previews \"Forms/Box\"]\nOne line of prose.\nAnd another.\n[preview \"A\"]\nbox\n    text \"a\"\n[play]\ncanvas.click(by_role(Role::Button))?;\ncanvas.expect_text(\"a\")?;\n";
    let folds = folding_ranges(src);
    assert!(has(&folds, 0, 2), "[view] stops at [previews]:\n{folds:?}");
    assert!(has(&folds, 3, 5), "[previews] prose:\n{folds:?}");
    assert!(has(&folds, 6, 8), "[preview] stops at [play]:\n{folds:?}");
    assert!(has(&folds, 9, 11), "[play]:\n{folds:?}");
}
