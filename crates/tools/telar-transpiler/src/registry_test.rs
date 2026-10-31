use super::*;

#[test]
fn builtin_and_control_flow_classification() {
    assert!(
        is_builtin_tag("col") && is_builtin_tag("text"),
        "the layout primitives are the registry's own"
    );
    assert!(!is_builtin_tag("feature_card"), "a user component is not");
    assert!(
        !is_builtin_tag("btn") && !is_builtin_tag("heading") && !is_builtin_tag("section"),
        "these ship as components, so the registry describes none of them"
    );
    assert!(
        is_control_flow_keyword("for") && !is_control_flow_keyword("col"),
        "`for` drives control flow; `col` is only a tag"
    );
}

#[test]
fn tag_attr_keys_layer_layout_and_tag_specific() {
    assert!(
        tag_attr_keys("col").contains(&"gap"),
        "a column takes the layout attributes"
    );
    assert!(
        tag_attr_keys("btn").is_empty(),
        "btn is a component, so the registry describes nothing for it"
    );
    assert!(
        tag_attr_keys("img").contains(&"src"),
        "an image takes its source"
    );
    let svg = tag_attr_keys("svg");
    assert!(
        svg.contains(&"src") && svg.contains(&"color") && svg.contains(&"gap"),
        "svg layers an image's src, a text's color and a container's gap: {svg:?}"
    );
    assert!(
        tag_attr_keys("feature_card").is_empty(),
        "a user component has no registry entry"
    );
    assert!(
        tag_attr_keys("box").contains(&"transition"),
        "transition is available wherever a box can be styled"
    );
    assert!(tag_attr_keys("text").contains(&"transition"), "and on text");
    assert!(
        tag_attr_keys("col").contains(&"transition"),
        "and on a column"
    );
    for tag in ["box", "col", "row", "grid"] {
        for spec in TRANSFORM_ATTRS {
            assert!(
                tag_attr_keys(tag).contains(&spec.key),
                "{tag} missing {}",
                spec.key
            );
        }
    }
}

#[test]
fn color_keywords_match_keyword_color_rgba() {
    assert_eq!(color_keywords(), &["transparent"]);
    assert_eq!(keyword_color_rgba("transparent"), Some([0, 0, 0, 0]));
    assert_eq!(keyword_color_rgba("cerulean"), None);
    assert_eq!(keyword_color_rgba("white"), None);
    assert_eq!(keyword_color_rgba("black"), None);
}

#[test]
fn color_keys_cover_every_attribute_that_paints() {
    for key in ["color", "fill", "stroke", "outline", "shadow_color"] {
        assert!(color_attr_keys().contains(&key), "missing {key}");
    }
    for key in ["gradient", "from", "to", "mid", "mid_pos", "radial_radius"] {
        assert!(!color_attr_keys().contains(&key), "{key} should be gone");
    }
}

/// Naming, language and hiding are generic: any tag that builds a box with a meaning of its own takes them, pictures included.
#[test]
fn every_meaningful_tag_takes_the_accessibility_attributes() {
    for tag in [
        "text", "col", "row", "grid", "box", "img", "image", "svg", "path", "canvas", "scroll",
        "input",
    ] {
        for key in ["label", "lang", "a11y"] {
            assert!(
                tag_attr_keys(tag).contains(&key),
                "`{tag}` should take `{key}`"
            );
        }
    }
    assert!(
        matches!(value_kind("text", "a11y"), Some(ValueKind::Keywords(table)) if keyword(table, "hidden") == Some("a11y_hidden"))
    );
}

/// `slot:` and a `<part>_style:` key are described where the editor asks, and a state key on a component call is not taken for a box's.
#[test]
fn a_call_s_slot_and_part_keys_are_described_and_its_state_keys_are_not() {
    assert!(attr_spec("card", SLOT_ATTR).is_some_and(|spec| spec.doc.is_some()));
    assert!(attr_spec("text", SLOT_ATTR).is_some_and(|spec| spec.doc.is_some()));
    assert!(attr_spec("sidebar", "header_style").is_some_and(|spec| spec.doc.is_some()));
    for key in STATE_STYLE_KEYS {
        assert!(attr_spec("sidebar", key).is_none(), "{key}");
        assert!(!is_part_style_key(key), "{key}");
    }
    assert!(attr_spec("box", "hover_style").is_some());
}
