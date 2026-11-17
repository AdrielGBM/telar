fn transpiled(view: &str) -> String {
    let src = format!("[logic]\nlet look = shown();\n[view]\n{view}");
    crate::transpile_source(&src, "demo", None, None)
        .unwrap()
        .rust_code
}

/// Each part of `look`, as a box written without `look:` would paint it.
const WRITTEN_OUT: &str = "fill:(::telar::Look::fill(&(look), $text.ink)) stroke:(::telar::Look::frame(&(look), $text.ink)) color:(::telar::Look::ink(&(look), $text.ink))";

const STATES_WRITTEN_OUT: &str = "hover_style(fill:(::telar::Look::hovered(&(look), $text.ink))) active_style(fill:(::telar::Look::pressed(&(look), $text.ink)))";

#[test]
fn a_look_paints_what_its_parts_written_out_paint() {
    assert_eq!(
        transpiled("row look:look stroke_width:2 radius:4\n    text \"x\"\n"),
        transpiled(&format!(
            "row stroke_width:2 radius:4 {WRITTEN_OUT}\n    text \"x\"\n"
        )),
    );
}

#[test]
fn on_a_box_that_answers_the_pointer_a_look_paints_its_states_too() {
    assert_eq!(
        transpiled("row look:look cursor:pointer\n    text \"x\"\n"),
        transpiled(&format!(
            "row cursor:pointer {WRITTEN_OUT} {STATES_WRITTEN_OUT}\n    text \"x\"\n"
        )),
    );
    let code = transpiled("row look:look role:button\n    text \"x\"\n");
    assert!(code.contains(".hover_style("), "{code}");
    assert!(code.contains(".active_style("), "{code}");
}

#[test]
fn a_look_never_makes_a_box_a_pointer_target() {
    let code = transpiled("row look:look\n    text \"x\"\n");
    assert!(!code.contains(".hover_style("), "{code}");
    assert!(!code.contains(".active_style("), "{code}");
}

#[test]
fn the_look_is_painted_in_the_ink_of_the_text_around_the_box() {
    let code = transpiled("row look:look\n    text \"x\"\n");
    assert!(
        code.contains("let __text = signal(BoxText::initial());"),
        "{code}"
    );
    assert!(code.contains(".following_text(__text)"), "{code}");
    assert!(
        code.contains(".with_fill(::telar::Look::fill(&(look), __text.get().ink))"),
        "{code}"
    );
    assert!(
        code.contains(".with_color(::telar::Look::ink(&(look), __text.get().ink))"),
        "{code}"
    );
}

#[test]
fn a_paint_attribute_the_box_writes_replaces_that_part_of_the_look() {
    let code = transpiled("row look:look fill:transparent color:$theme.text\n    text \"x\"\n");
    assert!(!code.contains("Look::fill("), "{code}");
    assert!(!code.contains("Look::ink("), "{code}");
    assert!(code.contains("Look::frame("), "{code}");
    assert_eq!(
        code,
        transpiled(
            "row look:look fill:transparent color:$theme.text stroke:(::telar::Look::frame(&(look), $text.ink))\n    text \"x\"\n"
        ),
    );
}

#[test]
fn a_state_style_the_box_writes_takes_the_look_s_fill_unless_it_names_one() {
    assert_eq!(
        transpiled("row look:look on_press:handler hover_style(radius:2)\n    text \"x\"\n"),
        transpiled(&format!(
            "row on_press:handler hover_style(radius:2 fill:(::telar::Look::hovered(&(look), $text.ink))) {WRITTEN_OUT} active_style(fill:(::telar::Look::pressed(&(look), $text.ink)))\n    text \"x\"\n"
        )),
    );
    let code =
        transpiled("row look:look on_press:handler hover_style(fill:#ff0000)\n    text \"x\"\n");
    assert!(!code.contains("Look::hovered("), "{code}");
    assert!(code.contains("Look::pressed("), "{code}");
}

#[test]
fn a_look_inside_a_loop_reads_its_value_afresh_in_every_part() {
    let code = transpiled(
        "col\n    for item in 0..3\n        row look:look cursor:pointer\n            text \"{item}\"\n",
    );
    assert!(!code.contains("compile_error!"), "{code}");
    assert!(code.contains("Look::pressed(&(look)"), "{code}");
}

#[test]
fn a_bare_look_names_no_value_and_is_refused() {
    let code = transpiled("row look\n    text \"x\"\n");
    assert!(
        code.contains("compile_error!(\"`look:` names the value the box is painted from"),
        "{code}"
    );
}

#[test]
fn only_a_box_takes_a_look() {
    let code = transpiled("text \"x\" look:look\n");
    assert!(
        code.contains("compile_error!(\"`look:` paints a `box`, `row`, `col` or `grid`, and `text` is none of them\")"),
        "{code}"
    );
}
