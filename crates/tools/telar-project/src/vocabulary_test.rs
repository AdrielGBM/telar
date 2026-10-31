use super::*;
use crate::naming::is_ident;

#[test]
fn every_word_names_one_value_of_one_axis() {
    let words: Vec<&str> = VOCABULARY
        .iter()
        .flat_map(|axis| axis.words)
        .copied()
        .collect();
    for (at, word) in words.iter().enumerate() {
        assert!(!words[..at].contains(word), "`{word}` is in two axes");
        assert_eq!(
            vocabulary_axis_of(word).map(|axis| axis.words.contains(word)),
            Some(true)
        );
    }
}

#[test]
fn a_word_is_a_lowercase_identifier_no_other_text_form_reads() {
    for word in VOCABULARY.iter().flat_map(|axis| axis.words) {
        assert!(is_ident(word), "`{word}`");
        assert!(word.chars().all(|c| c.is_ascii_lowercase()), "`{word}`");
        assert!(!matches!(*word, "none" | "true" | "false"), "`{word}`");
    }
}

#[test]
fn a_prop_finds_its_axis_by_name() {
    assert_eq!(
        vocabulary_axis("size").map(|axis| axis.words),
        Some(&["small", "regular", "large"][..])
    );
    assert_eq!(vocabulary_axis("label"), None);
    assert_eq!(
        vocabulary_axis_of("ghost").map(|axis| axis.name),
        Some("variant")
    );
    assert_eq!(vocabulary_axis_of("Ghost"), None);
}
