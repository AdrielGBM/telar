use super::*;

fn hover_text(src: &str, line: u32, character: u32) -> Option<String> {
    match hover_info(src, line, character, None)?.contents {
        HoverContents::Markup(markup) => Some(markup.value),
        _ => None,
    }
}

#[test]
fn keyword_color_hovers_a_swatch() {
    let src = "[view]\nbox fill:transparent\n";
    let text = hover_text(src, 1, 11).expect("hover over the `transparent` keyword");
    assert_eq!(text, "■ #00000000 — transparent");
}

#[test]
fn hex_literal_hovers_a_normalized_swatch() {
    let src = "[view]\nbox fill:#f0a\n";
    let text = hover_text(src, 1, 11).expect("hover over the hex literal");
    assert_eq!(text, "■ #ff00aa");
}

#[test]
fn a_layer_hovers_the_type_it_builds_and_what_it_is_for() {
    let src = "[view]\nlayer\n    text \"x\"\n";
    let text = hover_text(src, 1, 2).expect("hover over the tag");
    assert!(text.starts_with("`layer` → `FixedLayer::new()`"), "{text}");
    assert!(text.contains("Tab order"), "{text}");
}
