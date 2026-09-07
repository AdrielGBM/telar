use super::*;

/// The emitter and the tables were two lists of the same vocabulary, and they drifted: `aspect`, `aspect_ratio` and `flex_basis` were emitted by `layout_prop_call` for years while completion never offered them and the unknown-attribute check refused them. One table now, and this is what holds it to one.
#[test]
fn every_layout_key_offered_is_one_the_emitter_accepts() {
    let probe = |key: &str| match value_kind("box", key) {
        Some(ValueKind::Keywords(table)) | Some(ValueKind::KeywordsOrNumber(table)) => {
            table.first().map(|(name, _)| *name).unwrap_or("")
        }
        Some(ValueKind::Boolean) => "true",
        Some(ValueKind::Color) => "#000000",
        _ => "1",
    };
    for key in layout_attr_keys() {
        assert!(
            !matches!(
                crate::style::layout_prop_call(key, probe(key)),
                crate::style::PropCall::Invalid(_)
            ),
            "`{key}` is offered and the emitter refuses it"
        );
    }
    for key in ["aspect", "aspect_ratio", "flex_basis"] {
        assert!(
            matches!(
                crate::style::layout_prop_call(key, "1"),
                crate::style::PropCall::Call(_)
            ) && layout_attr_keys().contains(&key),
            "`{key}` drifted out of the table again"
        );
    }
}

/// A key is offered by completion and validated by the build off the same entry, so neither can name one the other does not.
#[test]
fn every_offered_key_resolves_to_its_own_spec() {
    for (tag, _) in builtin_tags() {
        for key in tag_attr_keys(tag) {
            assert!(
                attr_spec(tag, key).is_some(),
                "`{tag}` offers `{key}` and nothing describes it"
            );
        }
    }
}

/// One name is still two properties, and the per-tag tables are what keep them apart.
#[test]
fn stroke_means_what_the_tag_says_it_means() {
    assert!(
        matches!(value_kind("box", "stroke"), Some(ValueKind::Color)),
        "on a shape, stroke is a colour"
    );
    assert!(
        matches!(value_kind("svg", "stroke"), Some(ValueKind::Number)),
        "and its width is a length"
    );
    assert!(
        value_kind("path", "stroke").is_none(),
        "a path takes no stroke of its own"
    );
    assert!(
        matches!(value_kind("box", "stroke_width"), Some(ValueKind::Edges)),
        "an svg strokes like a shape"
    );
    assert!(
        value_kind("path", "stroke_width").is_none(),
        "and a path takes no stroke width either"
    );
}
