use super::*;

const LABELS: [&str; 4] = ["Delta", "Alpha", "Dune", "Disabled"];

fn find(search: &mut TypeAhead, c: char, from: Option<usize>) -> Option<usize> {
    search.extend(c).find(from, LABELS.len(), |i| {
        (LABELS[i] != "Disabled").then(|| LABELS[i].to_string())
    })
}

#[test]
fn a_repeated_letter_walks_the_rows_that_start_with_it() {
    let mut search = TypeAhead::default();
    assert_eq!(find(&mut search, 'd', None), Some(0));
    assert_eq!(find(&mut search, 'd', Some(0)), Some(2));
    assert_eq!(
        find(&mut search, 'd', Some(2)),
        Some(0),
        "past the last match it wraps, and a row the cursor may not stop on is passed over"
    );
}

#[test]
fn a_refined_query_holds_still_and_case_does_not_matter() {
    let mut search = TypeAhead::default();
    assert_eq!(find(&mut search, 'D', None), Some(0));
    assert_eq!(find(&mut search, 'u', Some(0)), Some(2));
    assert!(search.is_searching());
}

#[test]
fn nothing_matching_finds_nothing_and_an_empty_list_has_nothing_to_find() {
    let mut search = TypeAhead::default();
    assert_eq!(find(&mut search, 'z', Some(1)), None);
    assert_eq!(search.extend('a').find(None, 0, |_| None), None);
}
