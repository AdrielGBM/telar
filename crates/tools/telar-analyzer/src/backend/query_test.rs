use super::*;
use telar_transpiler::transpile_source;

/// The key position lands on the builder, so what comes back is the props struct's own setters rather than a table this crate would have to keep in step with every component in the workspace.
#[test]
fn an_attribute_key_maps_to_the_props_builder_that_carries_its_setter() {
    let rsx = "[logic]\nuse crate::ui::card::{CardProps, card};\n\n[view]\ncol\n    card pad:8\n";
    let out = transpile_source(rsx, "demo", None, None).unwrap();
    let map = SourceMap::new(out.source_map.clone(), out.expr_spans.clone());

    // `card pad:8` is the sixth line of the `.rsx`, zero-based 5.
    let offset = props_builder_offset(
        Section::View,
        rsx,
        &out.rust_code,
        &map,
        Position::new(5, 9),
    )
    .expect("the element's generated line carries a props builder");

    assert_eq!(
        &out.rust_code[offset - "CardProps::props().".len()..offset],
        "CardProps::props().",
        "the cursor sits where a method completion answers with the setters"
    );
}

/// A built-in tag never reaches here, and a line that produced no component call has no builder to point at — saying so is what keeps the caller on its own answer instead of a wrong offset.
#[test]
fn a_line_with_no_component_call_maps_nowhere() {
    let rsx = "[view]\ncol gap:8\n    text \"x\"\n";
    let out = transpile_source(rsx, "demo", None, None).unwrap();
    let map = SourceMap::new(out.source_map.clone(), out.expr_spans.clone());
    assert!(
        props_builder_offset(
            Section::View,
            rsx,
            &out.rust_code,
            &map,
            Position::new(1, 4)
        )
        .is_none(),
        "a line with no component call maps nowhere"
    );
}
