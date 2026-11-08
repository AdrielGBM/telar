use super::*;

#[test]
fn every_key_a_call_keeps_says_what_it_does() {
    for key in [A11Y_LABEL, LANG, A11Y, ROLE, SLOT, FORMER_LANG]
        .into_iter()
        .chain(STATE_STYLE_KEYS.iter().copied())
    {
        assert!(kept_by_the_call(key).is_some(), "`{key}`");
    }
}

#[test]
fn a_part_style_and_an_ordinary_prop_reach_the_component() {
    for key in ["label", "header_style", "style", "size", "language"] {
        assert!(kept_by_the_call(key).is_none(), "`{key}`");
    }
}
