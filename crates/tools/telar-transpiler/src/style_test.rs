use super::*;
use telar_parser::StyleProp;

fn call(key: &str, value: &str) -> Option<String> {
    match layout_prop_call(key, value) {
        PropCall::Call(call) => Some(call),
        _ => None,
    }
}

fn invalid(key: &str, value: &str) -> Option<String> {
    match layout_prop_call(key, value) {
        PropCall::Invalid(message) => Some(message),
        _ => None,
    }
}

#[test]
fn hex_expands() {
    assert_eq!(
        hex_to_color_expr("#3d78fa"),
        "Color::rgba(61.0 / 255.0, 120.0 / 255.0, 250.0 / 255.0, 255.0 / 255.0)"
    );
}

#[test]
fn width_prop() {
    assert_eq!(call("width", "240").as_deref(), Some(".width(240.0)"));
}

#[test]
fn logical_edge_props_map_to_their_builder_calls() {
    for (key, expected) in [
        ("pad_start", ".padding_start(12.0)"),
        ("padding_start", ".padding_start(12.0)"),
        ("pad_end", ".padding_end(12.0)"),
        ("margin_start", ".margin_inline_start(12.0)"),
        ("inset_end", ".inset_end(12.0)"),
    ] {
        assert_eq!(call(key, "12").as_deref(), Some(expected), "{key}");
    }
}

#[test]
fn direction_row_reverse_is_the_physical_one() {
    assert_eq!(call("axis", "row").as_deref(), Some(".flex_row()"));
    assert_eq!(
        call("axis", "row_reverse").as_deref(),
        Some(".flex_row_reverse()")
    );
}

/// The `theme` binding the view makes is read like any other reactive handle, in any numeric prop.
#[test]
fn a_theme_read_resolves_in_any_numeric_prop() {
    for (key, expected) in [
        ("pad", ".padding_all(theme.get().gutter)"),
        ("gap", ".gap(theme.get().gutter)"),
        ("width", ".width(theme.get().gutter)"),
        ("margin_start", ".margin_inline_start(theme.get().gutter)"),
    ] {
        assert_eq!(
            call(key, "$theme.gutter").as_deref(),
            Some(expected),
            "{key}"
        );
    }
}

/// A bare name is the author's own, whatever the theme happens to call a field of its own.
#[test]
fn a_bare_ident_is_the_name_the_author_wrote() {
    assert_eq!(
        call("pad", "card_gap").as_deref(),
        Some(".padding_all(card_gap)")
    );
}

#[test]
fn radius_is_ignored() {
    assert!(
        matches!(layout_prop_call("radius", "6"), PropCall::Other),
        "radius is painted, not laid out"
    );
}

#[test]
fn aspect_maps_to_aspect_ratio() {
    assert_eq!(call("aspect", "1").as_deref(), Some(".aspect_ratio(1.0)"));
    assert_eq!(
        call("aspect_ratio", "1.5").as_deref(),
        Some(".aspect_ratio(1.5)")
    );
}

/// A value the transpiler cannot resolve is the author's own Rust and reaches rustc, which names it against this `.rsx` line through the source map. There used to be a rejection here for anything that was not name-shaped, which is a judgement about Rust made by something that does not parse Rust.
#[test]
fn a_name_the_author_has_in_scope_is_carried_through() {
    for value in ["side", "props.pad", "crate::scale::md()", "TRACK_H"] {
        assert!(
            call("gap", value).is_some(),
            "`gap:{value}` names something the author has in scope"
        );
    }
}

/// A percentage resolves against the containing block wherever CSS says it does — every length, not the six size keys that happened to call the parser that knew about `%`.
#[test]
fn a_percentage_is_a_length_wherever_a_length_is() {
    for key in [
        "width",
        "pad",
        "padding_x",
        "gap",
        "margin_start",
        "inset_top",
    ] {
        assert!(
            call(key, "50%")
                .as_deref()
                .is_some_and(|c| c.contains("SizeDimension::Percent(0.5)")),
            "`{key}:50%` should resolve"
        );
    }
    for key in ["grow", "aspect"] {
        assert!(
            call(key, "50%")
                .as_deref()
                .is_some_and(|c| c.contains("SizeDimension::Percent(0.5)")),
            "`{key}:50%` expands and is rustc's to reject"
        );
    }
}

/// S3: a value outside a closed keyword set now says what the set is, on the attribute, instead of the property being dropped and the layout coming out subtly wrong.
#[test]
fn an_unknown_keyword_names_the_set_it_is_not_in() {
    let message = invalid("align", "centre").expect("`align:centre` should not compile");
    assert!(
        message.contains("`align:centre` is not a value of `align`"),
        "the message must name the bad value: {message}"
    );
    assert!(
        message.contains("`center`") && message.contains("`stretch`"),
        "and list what was allowed: {message}"
    );
    assert!(
        invalid("justify", "middle").is_some(),
        "an unknown justify value is rejected too"
    );
    assert!(invalid("axis", "sideways").is_some(), "and an unknown axis");
    assert!(
        invalid("self", "middle").is_some(),
        "and an unknown self alignment"
    );
}

/// A flag key still takes its bare form, and still rejects a spelled-out value it does not have.
#[test]
fn a_flag_key_keeps_its_bare_form() {
    assert_eq!(call("wrap", "").as_deref(), Some(".flex_wrap()"));
    assert_eq!(call("absolute", "").as_deref(), Some(".absolute()"));
    assert_eq!(
        call("absolute", "fill").as_deref(),
        Some(".absolute_fill()")
    );
    let message = invalid("absolute", "middle").expect("`absolute:middle` should not compile");
    assert!(message.contains("the bare flag"), "{message}");
}

/// A class property is written where no element is, so it has no attribute line — but it is the same misspelling, and it used to be dropped just as silently.
#[test]
fn a_class_property_reports_its_own_bad_value() {
    let section = StyleSection {
        classes: vec![StyleClass {
            name: "card".into(),
            props: vec![StyleProp {
                key: "align".into(),
                value: "centre".into(),
            }],
            line: 1,
        }],
    };
    let out = generate_style_section(&section, None);
    assert!(
        out.contains("compile_error!"),
        "a bad class property must fail the build: {out}"
    );
    assert!(out.contains("in `@card`"), "{out}");
}

/// A class property nobody recognises used to be dropped on the floor, which is how a renamed key goes on compiling and quietly stops laying the class out. Element attributes have been checked this way since keys were checked at all; classes never were.
#[test]
fn a_class_property_with_an_unknown_key_is_rejected() {
    let section = StyleSection {
        classes: vec![StyleClass {
            name: "card".into(),
            props: vec![StyleProp {
                key: "direction".into(),
                value: "col".into(),
            }],
            line: 1,
        }],
    };
    let out = generate_style_section(&section, None);
    assert!(out.contains("`direction` is not a style property"), "{out}");
}

/// A paint key is not a layout property and must not be mistaken for an unknown one: it reaches the `RectStyle` by another path entirely.
#[test]
fn a_paint_property_in_a_class_is_not_mistaken_for_an_unknown_key() {
    let section = StyleSection {
        classes: vec![StyleClass {
            name: "card".into(),
            props: vec![StyleProp {
                key: "fill".into(),
                value: "#fff".into(),
            }],
            line: 1,
        }],
    };
    assert!(
        !generate_style_section(&section, None).contains("compile_error!"),
        "a paint property belongs in a class: {}",
        generate_style_section(&section, None)
    );
}
