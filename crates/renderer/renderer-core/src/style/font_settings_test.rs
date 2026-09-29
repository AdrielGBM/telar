use super::*;

#[test]
fn a_tag_is_exactly_four_printable_characters() {
    assert_eq!(FontTag::parse("wght").map(|t| t.bytes()), Some(*b"wght"));
    assert_eq!(
        FontTag::parse("GRAD")
            .map(|t| t.as_str().to_owned())
            .as_deref(),
        Some("GRAD")
    );
    assert!(FontTag::parse("wgh").is_none());
    assert!(FontTag::parse("weight").is_none());
    assert!(FontTag::parse("wg\nt").is_none());
}

#[test]
fn settings_that_say_the_same_thing_are_equal_whatever_order_they_were_said_in() {
    let a = FontVariations::new()
        .with("wght", 650.0)
        .with("wdth", 110.0);
    let b = FontVariations::new()
        .with("wdth", 110.0)
        .with("wght", 650.0);
    assert_eq!(a, b);
    assert_eq!(a.key(), b.key());
    assert_eq!(
        a.with("wght", 700.0).get("wght"),
        Some(700.0),
        "a later value replaces the earlier"
    );
}

#[test]
fn settings_are_written_the_way_css_reads_them() {
    let axes = FontVariations::new()
        .with("wght", 650.0)
        .with("wdth", 112.5);
    assert_eq!(axes.to_css(), "\"wdth\" 112.5, \"wght\" 650");
    assert_eq!(FontVariations::new().to_css(), "normal");
    let features = FontFeatures::new().with("tnum", 1).with("liga", 0);
    assert_eq!(features.to_css(), "\"liga\" 0, \"tnum\" 1");
}

#[test]
fn axes_travel_one_by_one_and_an_unmatched_one_holds() {
    let from = FontVariations::new().with("wght", 400.0);
    let to = FontVariations::new().with("wght", 800.0).with("wdth", 75.0);
    let half = from.lerp(&to, 0.5);
    assert_eq!(half.get("wght"), Some(600.0));
    assert_eq!(half.get("wdth"), Some(75.0));
}
