use super::*;
use crate::context::{compute_layout, relayout_if_dirty, reset_layout_runtime, track_layout};
use crate::layout_item::LayoutItem;
use layout_core::AvailableSpace;
use reactive_core::signal;
use renderer_core::Color;

fn laid_height(text: &Text) -> f32 {
    let node = text.layout_node();
    compute_layout(
        node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::MaxContent,
    )
    .unwrap();
    track_layout(node).unwrap().get().height
}

fn paragraph(reading: Option<&'static str>) -> Text {
    let mut base = TextRun::new(|| "base".to_string());
    if let Some(reading) = reading {
        base = base.ruby(move || reading.to_string());
    }
    Text::runs(
        vec![base, TextRun::new(|| " and the rest".to_string())],
        LayoutStyle::new(),
        |_| TextStyle::new(20.0, Color::BLACK),
    )
    .unwrap()
}

#[test]
fn a_paragraph_holding_a_reading_takes_a_taller_box() {
    reset_layout_runtime();
    let plain = laid_height(&paragraph(None));
    reset_layout_runtime();
    let annotated = laid_height(&paragraph(Some("bs")));
    assert!(annotated > plain, "{annotated} over {plain}");
}

#[test]
fn the_box_grows_when_a_reading_appears() {
    reset_layout_runtime();
    let shown = signal(None::<String>);
    let text = Text::runs(
        vec![
            TextRun::new(|| "base".to_string()).ruby(move || shown.get()),
            TextRun::new(|| " and the rest".to_string()),
        ],
        LayoutStyle::new(),
        |_| TextStyle::new(20.0, Color::BLACK),
    )
    .unwrap();
    let hidden = laid_height(&text);
    shown.set(Some("bs".to_string()));
    relayout_if_dirty();
    let node = text.layout_node();
    assert!(track_layout(node).unwrap().get().height > hidden);
}

#[test]
fn the_drawn_line_box_holds_the_reading() {
    reset_layout_runtime();
    let text = paragraph(Some("bs"));
    let height = laid_height(&text);
    let drawn = drawn_line(&text.view()).expect("the text draws one paragraph");
    assert_eq!(drawn.height, height, "drawn in the box it was measured for");
}

fn drawn_line(node: &RenderNode) -> Option<geometry_core::Rect> {
    match node {
        RenderNode::Primitive(renderer_core::DrawCommand::Text { rect, .. }) => Some(*rect),
        RenderNode::Transform { children, .. }
        | RenderNode::Group { children }
        | RenderNode::Element { children, .. } => children.iter().find_map(drawn_line),
        _ => None,
    }
}
