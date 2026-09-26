use super::*;

#[test]
fn root_is_empty() {
    let loc = Location::root();
    assert!(loc.is_root());
    assert_eq!(loc.segments(), &[] as &[String]);
    assert_eq!(loc.fragment(), None);
    assert_eq!(loc.params(), &[]);
}

#[test]
fn builder_assembles_segments_fragment_and_params() {
    let loc = Location::root()
        .segment("acts")
        .segment("i")
        .with_fragment("diagram")
        .with_param("lang", "es");

    assert_eq!(loc.segments(), &["acts".to_string(), "i".to_string()]);
    assert_eq!(loc.fragment(), Some("diagram"));
    assert_eq!(loc.param("lang"), Some("es"));
    assert!(!loc.is_root());
}

#[test]
fn from_segments_matches_repeated_segment_calls() {
    let a = Location::from_segments(["acts", "i"]);
    let b = Location::root().segment("acts").segment("i");
    assert_eq!(a, b);
}

#[test]
fn repeated_params_keep_every_value_in_order() {
    let loc = Location::root()
        .with_param("tag", "a")
        .with_param("tag", "b");
    assert_eq!(
        loc.params(),
        &[
            ("tag".to_string(), "a".to_string()),
            ("tag".to_string(), "b".to_string()),
        ]
    );
    assert_eq!(
        loc.param("tag"),
        Some("a"),
        "the first value wins a lookup by key"
    );
}

#[test]
fn fragment_only_location_is_not_root() {
    let loc = Location::root().with_fragment("top");
    assert!(!loc.is_root());
    assert!(loc.segments().is_empty());
}
