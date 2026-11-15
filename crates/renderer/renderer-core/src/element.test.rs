use super::*;

const CARET: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 1.0,
    height: 16.0,
};

#[test]
fn a_selection_inside_a_character_moves_back_onto_its_start() {
    let editing = Editing::new("añb", 2, 3, CARET, false);
    assert_eq!((editing.anchor(), editing.focus()), (1, 3));
}

#[test]
fn a_selection_past_the_text_ends_with_it() {
    let editing = Editing::new("ab", 9, 1, CARET, false);
    assert_eq!((editing.anchor(), editing.focus()), (2, 1));
}
