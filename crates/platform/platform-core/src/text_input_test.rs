use super::*;

fn cursor_of(ime: Ime) -> Option<Range<usize>> {
    match ime {
        Ime::Preedit { cursor, .. } => cursor,
        other => panic!("not a composition: {other:?}"),
    }
}

#[test]
fn a_preedit_cursor_is_held_inside_its_text() {
    assert_eq!(cursor_of(Ime::preedit("かな", Some((0, 99)))), Some(0..6));
}

#[test]
fn a_preedit_cursor_reported_backwards_is_put_in_order() {
    assert_eq!(cursor_of(Ime::preedit("abc", Some((2, 1)))), Some(1..2));
}

#[test]
fn a_preedit_cursor_inside_a_character_moves_to_its_start() {
    assert_eq!(cursor_of(Ime::preedit("かな", Some((4, 4)))), Some(3..3));
}

#[test]
fn no_cursor_stays_no_cursor() {
    assert_eq!(cursor_of(Ime::preedit("abc", None)), None);
}

#[test]
fn a_browser_selection_is_counted_in_utf16_units() {
    assert_eq!(
        cursor_of(Ime::preedit_utf16("日本", Some((1, 2)))),
        Some(3..6)
    );
    assert_eq!(
        cursor_of(Ime::preedit_utf16("a😀b", Some((3, 3)))),
        Some(5..5)
    );
}

#[test]
fn a_browser_selection_past_the_end_is_the_end() {
    assert_eq!(
        cursor_of(Ime::preedit_utf16("ab", Some((7, 9)))),
        Some(2..2)
    );
}
