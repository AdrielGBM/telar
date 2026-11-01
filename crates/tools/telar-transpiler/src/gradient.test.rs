use super::*;

fn stops_of(value: &str) -> Vec<(f32, String)> {
    let (kind, args) = split_call(value).expect("a gradient call");
    parse(kind, args)
        .expect("a gradient")
        .stops
        .into_iter()
        .map(|(p, c)| (p, c.to_string()))
        .collect()
}

/// Two colours run the whole way, which is what `from`/`to` were.
#[test]
fn two_stops_sit_at_the_ends() {
    assert_eq!(
        stops_of("linear(#fff, #000)"),
        vec![(0.0, "#fff".into()), (1.0, "#000".into())]
    );
}

/// Three colours evenly spaced is `mid` with `mid_pos` left at its default, spelled by saying nothing.
#[test]
fn an_unpositioned_stop_takes_an_even_share() {
    assert_eq!(
        stops_of("linear(a, b, c)"),
        vec![(0.0, "a".into()), (0.5, "b".into()), (1.0, "c".into())]
    );
}

#[test]
fn a_stop_may_name_where_it_sits() {
    assert_eq!(
        stops_of("linear(a, b 0.45, c)"),
        vec![(0.0, "a".into()), (0.45, "b".into()), (1.0, "c".into())]
    );
}

/// The modifier is the first argument or absent, and a colour is never mistaken for one.
#[test]
fn a_leading_modifier_is_not_a_stop() {
    assert_eq!(stops_of("linear(horizontal, a, b)").len(), 2);
    assert_eq!(stops_of("radial(70, a, b)").len(), 2);
    assert_eq!(stops_of("radial(a, b)").len(), 2);
}

/// A colour that is itself a call has commas of its own, and they are not stop separators.
#[test]
fn a_call_valued_stop_reads_whole() {
    assert_eq!(
        stops_of("linear(chip(a, b), #000)"),
        vec![(0.0, "chip(a, b)".into()), (1.0, "#000".into())]
    );
}

#[test]
fn a_gradient_needs_two_stops() {
    let (kind, args) = split_call("linear(#fff)").expect("a call");
    assert!(
        parse(kind, args).is_none(),
        "a gradient with fewer than two stops is not one"
    );
    assert!(
        split_call("chip_fill($snap, id)").is_none(),
        "a call that is not a gradient does not split as one"
    );
}
