use super::*;

#[test]
fn tween_with_keyword_easing() {
    let (specs, errors) = parse_transition_value("opacity 200ms ease-out");
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].prop, "opacity");
    assert_eq!(
        specs[0].curve,
        "motion::tween(std::time::Duration::from_millis(200), motion::Easing::EaseOut)"
    );
}

#[test]
fn easing_defaults_to_ease_out_when_omitted() {
    let (specs, errors) = parse_transition_value("fill 150ms");
    assert!(
        errors.is_empty(),
        "an omitted easing is a default, not an error: {errors:?}"
    );
    assert_eq!(
        specs[0].curve,
        "motion::tween(std::time::Duration::from_millis(150), motion::Easing::EaseOut)"
    );
}

#[test]
fn seconds_duration_becomes_millis() {
    let (specs, _) = parse_transition_value("opacity 0.3s linear");
    assert_eq!(
        specs[0].curve,
        "motion::tween(std::time::Duration::from_millis(300), motion::Easing::Linear)"
    );
}

#[test]
fn cubic_bezier_and_spring() {
    let (cb, _) = parse_transition_value("fill 150ms cubic-bezier(0.4,0,0.2,1)");
    assert_eq!(
        cb[0].curve,
        "motion::tween(std::time::Duration::from_millis(150), motion::Easing::CubicBezier(0.4, 0.0, 0.2, 1.0))"
    );
    let (sp, _) = parse_transition_value("fill spring(170,26)");
    assert_eq!(sp[0].curve, "motion::spring(170.0, 26.0)");
}

#[test]
fn comma_separated_clauses_respect_parentheses() {
    let (specs, errors) =
        parse_transition_value("opacity 200ms cubic-bezier(0.4,0,0.2,1), fill 150ms linear");
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].prop, "opacity");
    assert_eq!(specs[1].prop, "fill");
}

#[test]
fn unsupported_property_and_bad_duration_report_errors() {
    let (specs, errors) = parse_transition_value("radius 200ms");
    assert!(specs.is_empty(), "a rejected declaration yields no spec");
    assert!(
        errors[0].contains("unsupported property `radius`"),
        "the error must name the property: {errors:?}"
    );

    let (_, errors) = parse_transition_value("opacity 200");
    assert!(
        errors[0].contains("invalid duration"),
        "the error must name the duration: {errors:?}"
    );
}
