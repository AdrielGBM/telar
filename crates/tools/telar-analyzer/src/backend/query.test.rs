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

/// The indent a verbatim line is given depends on where its zone lands, so the column shift is read off the two lines rather than assumed.
#[test]
fn a_logic_cursor_lands_on_the_same_text_in_the_generated_fn() {
    let rsx = "[logic]\nlet count = signal(0i32);\n\n[view]\ntext \"{$count}\"\n";
    let out = transpile_source(rsx, "demo", None, None).unwrap();
    let map = SourceMap::new(out.source_map.clone(), out.expr_spans.clone());
    let offset = generated_offset(
        Section::Logic,
        rsx,
        &out.rust_code,
        &map,
        Position::new(1, 4),
    )
    .expect("a [logic] line is mapped");
    assert!(out.rust_code[offset..].starts_with("count = signal"));
}

#[test]
fn a_play_cursor_maps_through_the_line_map_like_logic() {
    let rsx = "[view]\ncol\n\n[preview \"A\"]\ncol\n\n[play]\ncanvas.expect_text(\"x\")?;\n";
    let generated = "pub fn demo_play_0(canvas: &mut Play) -> PlayResult {\n        canvas.expect_text(\"x\")?;\n}\n";
    let map = SourceMap::new(vec![None, Some(7), None], Vec::new());
    let offset = generated_offset(Section::Play, rsx, generated, &map, Position::new(7, 7))
        .expect("a [play] line the output maps is mapped");
    assert!(generated[offset..].starts_with("expect_text"));

    let unmapped = SourceMap::new(vec![None, None, None], Vec::new());
    assert!(
        generated_offset(
            Section::Play,
            rsx,
            generated,
            &unmapped,
            Position::new(7, 7)
        )
        .is_none(),
        "a play the output does not carry maps nowhere"
    );
}
