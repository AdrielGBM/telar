use super::*;

#[test]
fn holds_a_multi_byte_cluster() {
    let g = Grapheme::new("é");
    assert_eq!(g.as_str(), "é");
}

#[test]
fn holds_a_zwj_sequence() {
    let family = "👨‍👩‍👧‍👦";
    assert!(
        family.len() <= 27,
        "fixture must fit inline: {}",
        family.len()
    );
    assert_eq!(Grapheme::new(family).as_str(), family);
}

#[test]
fn overlong_cluster_keeps_its_base_character() {
    let long = "a\u{0301}\u{0302}\u{0303}\u{0304}\u{0305}\u{0306}\u{0307}\u{0308}\u{0309}\u{030a}\u{030b}\u{030c}\u{030d}\u{030e}";
    assert!(long.len() > 27, "the cluster keeps its full text: {long}");
    assert_eq!(Grapheme::new(long).as_str(), "a");
}

#[test]
fn attrs_compose() {
    let a = Attrs::BOLD | Attrs::ITALIC;
    assert!(a.contains(Attrs::BOLD), "{a:?}");
    assert!(a.contains(Attrs::ITALIC), "{a:?}");
    assert!(
        !a.contains(Attrs::DIM),
        "an attribute that was never set must not appear: {a:?}"
    );
}
