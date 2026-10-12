//! Text placed in a stretched leaf under cell metrics, in a process of its own because the text metrics and layout grid are process-wide.

use geometry_core::{LayoutGrid, Size, set_layout_grid};
use layout_core::LayoutStyle;
use platform_core::Event;
use renderer_core::{Color, DrawCommand, TextStyle, set_text_metrics};
use renderer_tui::{CellMetrics, CellSize};
use telar_ui_core::{ComponentList, Text, WindowRoot, box_item};

fn text_tops(tree: &ComponentList) -> Vec<f32> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Text { rect, .. } => Some(rect.y),
            _ => None,
        })
        .collect()
}

#[test]
fn a_stretched_line_is_centred_in_a_taller_box_by_whole_rows() {
    set_text_metrics(CellMetrics::new(CellSize::default()));
    set_layout_grid(LayoutGrid::new(8.0, 16.0));
    telar_ui_core::set_surface_size(Size::new(160.0, 80.0));
    let text = Text::new(
        || "Setup".to_string(),
        LayoutStyle::new().width(160.0).height(80.0),
        || TextStyle::new(14.0, Color::WHITE),
    )
    .unwrap();
    let mut tree = ComponentList::new(WindowRoot::new(box_item(text)));
    tree.on_event(&Event::WindowResized {
        width: 160,
        height: 80,
    });
    assert_eq!(text_tops(&tree), vec![32.0]);
}
