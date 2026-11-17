use crate::{RsxSpan, SourceMap, transpile_source};

const LOGIC: &str = "[logic]\npub struct Props {\n    pub size: f32,\n}\n\nlet size = props.size;\nlet count = signal(0.0);\n\n";

fn transpiled(style: &str, view: &str) -> crate::TranspiledSource {
    let source = format!("{LOGIC}[style]\n{style}\n[view]\n{view}\n");
    transpile_source(&source, "demo", None, None).expect("transpiles")
}

/// The Rust `view` generates under `style`, which names a class some elements of `view` use.
fn emitted(style: &str, view: &str) -> String {
    transpiled(style, view).rust_code
}

/// Asserts that naming the class in `style` on an element is the same as writing its attributes there, the way `inline` does.
fn assert_class_is_inline(style: &str, classed: &str, inline: &str) {
    assert_eq!(
        emitted(style, classed),
        emitted(style, inline),
        "`{classed}` should emit what `{inline}` does"
    );
}

#[test]
fn a_class_lays_out_as_its_attributes_written_inline() {
    assert_class_is_inline(
        "@bar\n    gap: 8\n    pad_x: 4\n    align: center\n    grow: 1",
        "row @bar\n    text \"a\"",
        "row gap:8 pad_x:4 align:center grow:1\n    text \"a\"",
    );
}

#[test]
fn a_class_paints_as_its_attributes_written_inline() {
    assert_class_is_inline(
        "@card\n    fill: #ff0000\n    stroke: #000000\n    radius: 8\n    shadow_blur: 4",
        "box @card",
        "box fill:#ff0000 stroke:#000000 radius:8 shadow_blur:4",
    );
}

/// A state style keeps the base paint it does not name, so the class's radius rounds the hovered box too.
#[test]
fn a_class_s_state_style_is_the_inline_one_and_keeps_the_base_paint() {
    let style = "@card\n    fill: #ff0000\n    radius: 8\n    hover_style: fill:#00ff00";
    assert_class_is_inline(
        style,
        "box @card",
        "box fill:#ff0000 radius:8 hover_style(fill:#00ff00)",
    );
    let code = emitted(style, "box @card");
    let hover = &code[code.find(".hover_style(").expect("a hover style")..];
    assert!(
        hover.contains("BorderRadius::all(8.0)"),
        "the hovered box keeps the class's radius:\n{code}"
    );
}

#[test]
fn a_class_reads_the_component_s_logic_as_an_inline_value_does() {
    let style = "@gapped\n    gap: (size * 2.0)\n    pad: (props.size)";
    assert_class_is_inline(
        style,
        "col @gapped",
        "col gap:(size * 2.0) pad:(props.size)",
    );
    let code = emitted(style, "col @gapped");
    let component = &code[code.find("pub fn demo(").expect("the component")..];
    assert!(component.contains("size * 2.0"), "{code}");
    assert!(
        !code.contains("compile_error!"),
        "a private class reads its own component:\n{code}"
    );
}

#[test]
fn a_class_follows_state_as_an_inline_value_does() {
    let style = "@padded\n    pad: $count\n    fill: (if $count > 1.0 { Color::WHITE } else { Color::BLACK })";
    assert_class_is_inline(
        style,
        "box @padded",
        "box pad:$count fill:(if $count > 1.0 { Color::WHITE } else { Color::BLACK })",
    );
    let code = emitted(style, "box @padded");
    assert!(
        code.contains(".styled_by(") && code.contains("count.get()"),
        "the layout is re-resolved as the count changes:\n{code}"
    );
}

#[test]
fn an_attribute_written_on_the_element_overrides_its_class() {
    let style = "@card\n    fill: #ff0000\n    radius: 8\n    pad: 8";
    assert_class_is_inline(
        style,
        "box @card fill:#0000ff pad:2",
        "box radius:8 fill:#0000ff pad:2",
    );
    let code = emitted(style, "box @card fill:#0000ff pad:2");
    assert!(
        code.contains("BorderRadius::all(8.0)"),
        "the class's own paint stays when the element paints too:\n{code}"
    );
    assert!(
        !code.contains("255.0 / 255.0, 0.0 / 255.0, 0.0 / 255.0"),
        "{code}"
    );
}

#[test]
fn a_later_class_overrides_an_earlier_one() {
    assert_class_is_inline(
        "@a\n    fill: #ff0000\n    gap: 2\n\n@b\n    fill: #00ff00",
        "box @a @b",
        "box gap:2 fill:#00ff00",
    );
}

#[test]
fn a_class_on_a_span_is_its_attributes_written_on_the_span() {
    assert_class_is_inline(
        "@strong\n    font_weight: bold\n    color: #ff0000",
        "text\n    span @strong \"a\"",
        "text\n    span font_weight:bold color:#ff0000 \"a\"",
    );
}

#[test]
fn a_class_property_the_tag_does_not_take_is_refused_on_the_element() {
    let code = emitted("@card\n    fill: #ff0000", "text @card \"a\"");
    assert!(
        code.contains("compile_error!(\"in `@card`: `fill` is not an attribute of `text`\")"),
        "{code}"
    );
}

/// A length in `lh` is one of the text of the box it lands on, which only an element naming the class has.
#[test]
fn a_class_length_in_the_box_s_text_is_judged_on_the_element() {
    let style = "@cap\n    min_width: 1lh";
    let boxed = emitted(style, "row @cap");
    assert!(!boxed.contains("compile_error!"), "{boxed}");
    assert!(boxed.contains("__text.get().lh"), "{boxed}");
    let text = emitted(style, "text @cap \"a\"");
    assert!(
        text.contains("in `@cap`: `min_width:1lh` on `text`"),
        "{text}"
    );
}

/// A rustc error in a class value lands on the element naming the class, never on whatever text sits at the value's offset in another place.
#[test]
fn a_class_value_maps_to_the_element_naming_it() {
    let out = transpiled("@card\n    fill: accent", "box @card");
    let source = format!("{LOGIC}[style]\n@card\n    fill: accent\n[view]\nbox @card\n");
    let at = out
        .rust_code
        .find("Handed(accent)")
        .expect("the class's fill")
        + "Handed(".len();
    let map = SourceMap::new(out.source_map.clone(), out.expr_spans.clone());
    let element_line = source.lines().position(|line| line == "box @card").unwrap() as u32;
    assert_eq!(
        map.locate(&out.rust_code, at as u32, at as u32 + 6, &source),
        Some(RsxSpan::Line(element_line))
    );
}

#[test]
fn a_shared_class_reading_the_component_is_refused_on_its_line() {
    let out = transpiled(
        "pub @shared\n    gap: (size * 2.0)\n    pad: (props.size)",
        "col @shared",
    );
    let at = out
        .rust_code
        .lines()
        .position(|line| line.contains("`pub @shared` reads `props`, `size`"))
        .unwrap_or_else(|| panic!("the class is refused:\n{}", out.rust_code));
    let source = format!("{LOGIC}[style]\n");
    assert_eq!(out.source_map[at], Some(source.lines().count() as u32));

    let private = emitted("@own\n    gap: (size * 2.0)", "col @own");
    assert!(!private.contains("compile_error!"), "{private}");
    let reachable = emitted("pub @shared\n    gap: (GAP * 2.0)", "col @shared");
    assert!(!reachable.contains("compile_error!"), "{reachable}");
}

#[test]
fn a_class_on_a_component_call_amends_its_surface_and_reads_the_component() {
    let code = emitted(
        "@round\n    radius: (size)\n    fill: #ff0000",
        "menu @round label:\"File\" items:items",
    );
    let call = &code[code.find("menu(MenuProps::props()").expect("the call")..];
    assert!(
        call.contains(".style(std::rc::Rc::new(")
            && call.contains(".with_radius(BorderRadius::all(size))"),
        "{code}"
    );
    assert!(!code.contains("RectStyle {"), "{code}");
}
