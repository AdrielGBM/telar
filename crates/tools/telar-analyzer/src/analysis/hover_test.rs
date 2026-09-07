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
