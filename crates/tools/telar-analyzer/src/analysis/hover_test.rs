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

#[test]
fn a_preview_option_hovers_what_it_does() {
    let src = "[view]\ncol\n\n[preview \"A\" layout:centered args(on:true)]\ncol\n";
    let layout = hover_text(src, 3, 15).expect("hover over `layout`");
    assert!(layout.starts_with("`layout` — preview option"), "{layout}");
    assert!(layout.contains("centered"), "{layout}");
    let args = hover_text(src, 3, 31).expect("hover over `args`");
    assert!(args.contains("signals"), "{args}");
    assert!(hover_text(src, 3, 23).is_none(), "a value says nothing");
}

#[test]
fn a_component_calls_role_hovers_only_the_roles_a_call_may_say() {
    let src = "[view]\ncard role:nav\n";
    let text = hover_text(src, 1, 6).expect("hover over `role`");
    assert!(text.starts_with("`role` — group | banner"), "{text}");
    assert!(text.contains("listitem"), "{text}");
    assert!(!text.contains("button"), "{text}");
    let builtin = hover_text("[view]\nbox role:nav\n", 1, 5).expect("hover over a box's `role`");
    assert!(builtin.contains("button"), "{builtin}");
}
