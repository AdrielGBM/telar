use super::*;

fn preedit(text: &str, cursor: Option<Range<usize>>) -> Preedit {
    Preedit {
        text: text.to_string(),
        cursor,
    }
}

#[test]
fn a_composition_is_drawn_at_the_caret() {
    let composed = compose("ab", 1, &preedit("かな", Some(6..6)));
    assert_eq!(composed.text, "aかなb");
    assert_eq!(composed.span, 1..7);
    assert_eq!(composed.caret, Some(7));
    assert_eq!(composed.converting, None);
    assert_eq!(composed.anchor, 7);
}

#[test]
fn a_range_cursor_is_the_part_being_converted_and_no_caret() {
    let composed = compose("", 0, &preedit("日本語", Some(3..9)));
    assert_eq!(composed.caret, None);
    assert_eq!(composed.converting, Some(3..9));
    assert_eq!(composed.anchor, 3);
}

#[test]
fn no_cursor_puts_the_candidates_after_the_composition() {
    let composed = compose("x", 1, &preedit("ka", None));
    assert_eq!((composed.caret, composed.converting), (None, None));
    assert_eq!(composed.anchor, 3);
}

#[test]
fn text_after_the_caret_moves_past_the_composition() {
    let composed = compose("abcd", 2, &preedit("xyz", None));
    assert_eq!(composed.shift(1), 1);
    assert_eq!(composed.shift(2), 2);
    assert_eq!(composed.shift(3), 6);
}

#[test]
fn the_first_text_begins_a_composition_and_the_rest_change_it() {
    let composition = Composition::new();
    assert_eq!(composition.apply(&Ime::preedit("k", None)), Step::Began);
    assert_eq!(composition.apply(&Ime::preedit("か", None)), Step::Changed);
    assert!(composition.is_active());
}

#[test]
fn an_empty_composition_ends_one_in_progress_and_ignores_none() {
    let composition = Composition::new();
    assert_eq!(composition.apply(&Ime::preedit("", None)), Step::Ignored);
    composition.apply(&Ime::preedit("k", None));
    assert_eq!(composition.apply(&Ime::preedit("", None)), Step::Ended);
    assert!(!composition.is_active());
}

#[test]
fn a_commit_ends_the_composition_with_its_text() {
    let composition = Composition::new();
    composition.apply(&Ime::preedit("か", None));
    assert_eq!(
        composition.apply(&Ime::Commit("可".into())),
        Step::Committed("可".into())
    );
    assert!(!composition.is_active());
}

#[test]
fn the_input_method_turning_off_takes_its_composition_with_it() {
    let composition = Composition::new();
    composition.apply(&Ime::preedit("か", None));
    assert_eq!(composition.apply(&Ime::Disabled), Step::Ended);
    assert_eq!(composition.apply(&Ime::Enabled), Step::Ignored);
}
