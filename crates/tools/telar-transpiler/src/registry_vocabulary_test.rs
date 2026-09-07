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

/// The other half of the table, which had no guard at all.
///
/// [`builtin_tags`] is what downstream tooling reads to answer "is this a widget or one of yours" — completion offers from it, the unknown-attribute check validates against it, hover documents from it. The emitter answers the same question from its own `match`, and a tag in one and not the other fails in the direction that hurts: `emit_element_inner` falls through to `emit_component_call`, so a built-in the table names but the emitter forgot compiles to a call to a function nobody wrote — and rustc reports an undefined name for a tag the editor had just offered.
///
/// The layout-attribute half of this drifted for years before anyone noticed (see above). This is the same guard for tags.
///
/// It asserts the **type**, not the exact constructor. The column spells a canonical one, and for three tags the emitter picks a sibling of it: `text`, `input` and `path` compile to `::declaring` when a value is reactive or a style is inherited, which for `text` is essentially always. Which constructor of a type codegen reaches for is a property of the values on the line, not of the tag — where the tag's identity ends is the type, and that is what the table can promise and hover shows.
#[test]
fn every_tag_the_table_names_builds_the_type_it_promises() {
    for (tag, constructor) in builtin_tags() {
        if *constructor == TAG_SLOT_PLACEHOLDER {
            // Builds nothing by definition: it splices the caller's children into the enclosing container.
            continue;
        }
        let ty = constructor
            .split("::")
            .next()
            .expect("a constructor path names a type");
        let source = format!("[view]\n{}\n", minimal_use(tag));
        let out = crate::transpile_source(&source, "demo", None, None)
            .unwrap_or_else(|e| panic!("`{tag}` is in the table and does not transpile: {e:?}"));
        assert!(
            out.rust_code.contains(&format!("{ty}::")),
            "`{tag}` is offered as a built-in that builds a `{ty}`, and the emitter produced something \
             else — a tag the emitter does not know falls through to a component call:\n{}",
            out.rust_code
        );
        assert!(
            !out.rust_code.contains("compile_error!"),
            "`{tag}` in its minimal form should transpile clean; either the emitter refuses a built-in or \
             this test does not write the tag the way the emitter requires:\n{}",
            out.rust_code
        );
    }
}

/// The smallest use of each tag that the emitter accepts. Tags needing no attribute are written bare with one child; the rest carry exactly what the emitter refuses to run without, which is itself worth pinning — a built-in that grows a required attribute makes this fail rather than silently accepting less.
fn minimal_use(tag: &str) -> String {
    match tag {
        "text" => "text \"x\"".to_string(),
        "path" => "path d:\"M0 0 L1 1\"".to_string(),
        "canvas" => "canvas paint:(|_rect| ())".to_string(),
        "lazy" => "lazy when:true\n    text \"x\"".to_string(),
        _ => format!("{tag}\n    text \"x\""),
    }
}

/// And the fallback the test above leans on: a tag the table does not name is somebody's component, not an error.
#[test]
fn a_tag_the_table_does_not_name_is_a_component_call() {
    let out = crate::transpile_source("[view]\nmy_widget\n", "demo", None, None).unwrap();
    assert!(
        out.rust_code.contains("my_widget("),
        "an unknown tag is somebody's component:\n{}",
        out.rust_code
    );
    assert!(!is_builtin_tag("my_widget"));
}
