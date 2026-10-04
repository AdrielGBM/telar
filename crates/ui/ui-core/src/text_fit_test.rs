use layout_core::{AlignItems, AvailableSpace, LayoutStyle};
use reactive_core::signal;
use renderer_core::{Color, DrawCommand, FontFit, TextLength, TextStyle};
use ui_tree::{Component, RenderNode};

use crate::context::{compute_layout, new_container, reset_layout_runtime, track_layout};
use crate::layout_item::LayoutItem;
use crate::text::Text;

fn drawn_style(node: &RenderNode) -> Option<TextStyle> {
    match node {
        RenderNode::Primitive(DrawCommand::Text { style, .. }) => Some((**style).clone()),
        RenderNode::Transform { children, .. }
        | RenderNode::Group { children }
        | RenderNode::Element { children, .. } => children.iter().find_map(drawn_style),
        _ => None,
    }
}

fn line_width(content: &str, style: &TextStyle) -> f32 {
    crate::text_metrics::measure_text(content, None, 1.0e6, style).0
}

fn lay_out(root: layout_core::NodeId, width: f32) {
    compute_layout(
        root,
        AvailableSpace::Definite(width),
        AvailableSpace::Definite(800.0),
    )
    .unwrap();
}

fn fitted(fit: FontFit) -> Text {
    Text::fitting(
        || "ADRIEL".to_string(),
        LayoutStyle::new(),
        move || fit,
        |inherited: TextStyle| {
            inherited
                .with_color(Color::BLACK)
                .with_letter_spacing_in(TextLength::Em(-0.04), geometry_core::Size::ZERO)
                .with_line_height(0.84)
        },
    )
    .unwrap()
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.5
}

#[test]
fn a_fitted_line_comes_to_its_share_of_the_box_holding_it_and_follows_it() {
    reset_layout_runtime();
    let text = fitted(FontFit::containing(0.6));
    let root = new_container(
        LayoutStyle::new().flex_column().padding_all(20.0),
        &[text.layout_node()],
    )
    .unwrap();
    for width in [840.0_f32, 440.0, 1240.0] {
        lay_out(root, width);
        let style = drawn_style(&text.view()).expect("a text draws text");
        let target = 0.6 * (width - 40.0);
        assert!(
            close(line_width("ADRIEL", &style), target),
            "at {width}: the line is {} wide at {}px, asked for {target}",
            line_width("ADRIEL", &style),
            style.font_size
        );
        assert!(
            (style.letter_spacing + 0.04 * style.font_size).abs() < 1.0e-3,
            "tracking in em scales with the fitted size"
        );
        let rect = track_layout(text.layout_node()).unwrap().get();
        assert!(
            close(rect.height, 0.84 * style.font_size),
            "the box is measured at the size the line is drawn at: {rect:?} at {}",
            style.font_size
        );
    }
}

#[test]
fn a_fitted_line_in_a_row_is_its_share_of_the_row_not_of_itself() {
    reset_layout_runtime();
    let text = fitted(FontFit::containing(0.5));
    let root = new_container(
        LayoutStyle::new().flex_row().align_items(AlignItems::START),
        &[text.layout_node()],
    )
    .unwrap();
    lay_out(root, 600.0);
    let rect = track_layout(text.layout_node()).unwrap().get();
    assert!(close(rect.width, 300.0), "{rect:?}");
}

#[test]
fn a_maximum_height_holds_a_wide_line_down() {
    reset_layout_runtime();
    let text = fitted(FontFit::containing(1.0).with_max_height(84.0));
    let root = new_container(LayoutStyle::new().flex_column(), &[text.layout_node()]).unwrap();
    lay_out(root, 2000.0);
    let style = drawn_style(&text.view()).expect("a text draws text");
    assert!(close(style.font_size, 100.0), "{}", style.font_size);
    assert!(close(
        track_layout(text.layout_node()).unwrap().get().height,
        84.0
    ));
}

#[test]
fn a_fit_that_changes_fits_the_line_again() {
    reset_layout_runtime();
    let share = signal(0.6_f32);
    let text = Text::fitting(
        || "BARRIENTOS".to_string(),
        LayoutStyle::new(),
        move || FontFit::containing(share.get()),
        |inherited: TextStyle| inherited.with_color(Color::BLACK),
    )
    .unwrap();
    let root = new_container(LayoutStyle::new().flex_column(), &[text.layout_node()]).unwrap();
    lay_out(root, 1000.0);
    let wide = drawn_style(&text.view()).unwrap().font_size;
    share.set(0.3);
    crate::context::relayout_if_dirty();
    let narrow = drawn_style(&text.view()).unwrap();
    assert!(
        close(line_width("BARRIENTOS", &narrow), 300.0),
        "{} at {}",
        line_width("BARRIENTOS", &narrow),
        narrow.font_size
    );
    assert!(
        close(narrow.font_size * 2.0, wide),
        "{} vs {wide}",
        narrow.font_size
    );
}

#[test]
fn a_fitted_line_never_wraps() {
    reset_layout_runtime();
    let text = Text::fitting(
        || "TWO WORDS".to_string(),
        LayoutStyle::new(),
        || FontFit::containing(1.0),
        |inherited: TextStyle| inherited.with_color(Color::BLACK),
    )
    .unwrap();
    let root = new_container(LayoutStyle::new().flex_column(), &[text.layout_node()]).unwrap();
    lay_out(root, 500.0);
    let style = drawn_style(&text.view()).unwrap();
    assert_eq!(style.text_wrap, renderer_core::TextWrap::NoWrap);
    let line = crate::text_metrics::line_box(&style);
    assert!(
        track_layout(text.layout_node()).unwrap().get().height <= line + 0.5,
        "a line fitted to the whole width must not break onto a second"
    );
}
