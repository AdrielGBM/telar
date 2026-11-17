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

/// A call places the item a component returns with these, so no prop could take one.
#[test]
fn a_call_keeps_the_keys_that_place_the_item() {
    for key in [
        "width",
        "grow",
        "self",
        "margin_top",
        "inset_start",
        "shown",
    ] {
        assert!(kept_by_the_call(key).is_some(), "{key}");
    }
    for key in ["pad", "gap", "align", "wrap"] {
        assert!(
            kept_by_the_call(key).is_none(),
            "`{key}` is how the component lays out what it holds, its own"
        );
    }
}
