use super::*;
use telar_parser::Value;

fn attr(key: &str, value: &str) -> Attr {
    Attr {
        key: key.to_string(),
        value: Value::Expr(value.to_string()),
        value_start: 0,
    }
}

#[test]
fn shorthand_follows_the_css_arities() {
    assert_eq!(expand_shorthand("1"), Some(["1", "1", "1", "1"]));
    assert_eq!(expand_shorthand("1 2"), Some(["1", "2", "1", "2"]));
    assert_eq!(expand_shorthand("1 2 3"), Some(["1", "2", "3", "2"]));
    assert_eq!(expand_shorthand("1 2 3 4"), Some(["1", "2", "3", "4"]));
    assert_eq!(expand_shorthand(""), None);
    assert_eq!(expand_shorthand("1 2 3 4 5"), None);
}

/// The case the whole feature exists for, and the one that has to stay one attribute long.
#[test]
fn a_single_side_leaves_every_other_side_at_nothing() {
    let edges = collect(
        &[attr("stroke_width", "0 0 1 0")],
        "stroke_width",
        "stroke_",
        side_target,
    );
    assert!(
        edges.uniform.is_none(),
        "four tokens is not the uniform form"
    );
    assert_eq!(
        edges.resolved("0.0"),
        ["0.0", "0.0", "1.0", "0.0"].map(String::from)
    );
}

/// A plain width has to keep emitting nothing per-side, or every box in every existing app grows four numbers it never asked for.
#[test]
fn a_plain_width_stays_uniform() {
    let edges = collect(
        &[attr("stroke_width", "2")],
        "stroke_width",
        "stroke_",
        side_target,
    );
    assert_eq!(edges.uniform.as_deref(), Some("2.0"));
}

/// `stroke_width` itself starts with the side prefix, and `width` is not a side.
#[test]
fn the_base_key_is_not_mistaken_for_one_of_its_own_sides() {
    let edges = collect(
        &[attr("stroke_width", "2"), attr("stroke_bottom", "1")],
        "stroke_width",
        "stroke_",
        side_target,
    );
    assert_eq!(
        edges.resolved("0.0"),
        ["2.0", "2.0", "1.0", "2.0"].map(String::from),
        "the shorthand seeds all four and the named side overrides its own"
    );
}

/// Written the other way round, the result is the same: specificity decides, not source order.
#[test]
fn a_named_edge_beats_the_shorthand_whichever_came_first() {
    let written_backwards = collect(
        &[attr("radius_top", "0"), attr("radius", "8")],
        "radius",
        "radius_",
        corner_target,
    );
    assert_eq!(
        written_backwards.resolved("0.0"),
        ["0.0", "0.0", "8.0", "8.0"].map(String::from)
    );
}

#[test]
fn a_pair_beats_the_shorthand_and_a_single_corner_beats_the_pair() {
    let edges = collect(
        &[
            attr("radius", "8"),
            attr("radius_top", "4"),
            attr("radius_top_left", "0"),
        ],
        "radius",
        "radius_",
        corner_target,
    );
    assert_eq!(
        edges.resolved("0.0"),
        ["0.0", "4.0", "8.0", "8.0"].map(String::from)
    );
}

#[test]
fn logical_edges_are_kept_apart_for_the_direction_to_resolve() {
    let edges = collect(
        &[attr("stroke_end", "1")],
        "stroke_width",
        "stroke_",
        side_target,
    );
    assert!(
        edges.has_logical(),
        "a logical edge stays logical until the direction resolves it"
    );
    assert_eq!(edges.logical_args(), ("None".into(), "Some(1.0)".into()));
}

#[test]
fn nothing_written_is_empty() {
    let edges = collect(&[attr("fill", "ink")], "radius", "radius_", corner_target);
    assert!(edges.is_empty(), "nothing written leaves the set empty");
}
