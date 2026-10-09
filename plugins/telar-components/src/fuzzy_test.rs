use super::*;

fn score(query: &str, text: &str) -> i32 {
    fuzzy_match(query, text)
        .unwrap_or_else(|| panic!("`{query}` matches `{text}`"))
        .score
}

fn ranges(query: &str, text: &str) -> Vec<Range<usize>> {
    fuzzy_match(query, text).expect("a match").ranges
}

#[test]
fn every_character_has_to_appear_in_order() {
    assert!(fuzzy_match("tgl", "Toggle").is_some());
    assert!(fuzzy_match("lgt", "Toggle").is_none());
    assert!(fuzzy_match("toggles", "Toggle").is_none());
}

#[test]
fn case_and_the_query_s_spaces_do_not_matter() {
    assert!(fuzzy_match("OPEN FILE", "open file").is_some());
    assert!(fuzzy_match("inputs button", "Inputs/Button").is_some());
}

#[test]
fn an_empty_query_matches_everything_with_nothing_marked() {
    assert_eq!(
        fuzzy_match("  ", "Anything"),
        Some(FuzzyMatch {
            score: 0,
            ranges: Vec::new()
        })
    );
}

#[test]
fn a_run_of_characters_beats_the_same_characters_scattered() {
    assert!(score("but", "Button") > score("but", "Bottom utility"));
}

#[test]
fn the_start_of_a_word_beats_its_middle() {
    assert!(score("b", "Toggle button") > score("b", "Rubber"));
    assert!(score("tv", "tree_view") > score("tv", "outvote"));
    assert!(score("tv", "TreeView") > score("tv", "outvote"));
}

#[test]
fn less_skipped_text_scores_higher() {
    assert!(score("save", "Save") > score("save", "Show all versions everywhere"));
}

#[test]
fn the_best_alignment_is_the_one_marked() {
    assert_eq!(ranges("tv", "tab view"), vec![0..1, 4..5]);
    assert_eq!(ranges("view", "preview view"), vec![8..12]);
}

#[test]
fn marks_are_byte_ranges_of_the_text() {
    assert_eq!(ranges("é", "café"), vec![3..5]);
    assert_eq!(ranges("caf", "Café"), vec![0..3]);
    assert!(fuzzy_match("cafe", "Café").is_none());
}
