//! Text placed in a stretched leaf under cell metrics, in a process of its own because the text metrics and layout grid are process-wide. A terminal draws each glyph inside its cell, so the band a line is centred on is the whole row.

use geometry_core::{LayoutGrid, Rect, Size, set_layout_grid};
use layout_core::LayoutStyle;
use platform_core::Event;
use renderer_core::{Color, DrawCommand, TextStyle, set_text_metrics};
use renderer_tui::{CellMetrics, CellSize};
use telar_ui_core::{ComponentList, Container, Input, LayoutItem, Text, WindowRoot, box_item};

const ROW: f32 = 16.0;

fn lines(tree: &ComponentList) -> Vec<Rect> {
    let mut lines = Vec::new();
    renderer_core::for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let DrawCommand::Text { rect, .. } = command {
            lines.push(Rect {
                x: matrix[4] + rect.x,
                y: matrix[5] + rect.y,
                ..*rect
            });
        }
    });
    lines
}

fn on_cells() {
    set_text_metrics(CellMetrics::new(CellSize::default()));
    set_layout_grid(LayoutGrid::new(8.0, ROW));
}

fn label(content: &'static str, layout: LayoutStyle) -> Text {
    on_cells();
    Text::new(
        move || content.to_string(),
        layout,
        || TextStyle::new(14.0, Color::WHITE),
    )
    .unwrap()
}

fn drawn(width: f32, height: f32, root: Box<dyn LayoutItem>) -> ComponentList {
    on_cells();
    telar_ui_core::set_surface_size(Size::new(width, height));
    let mut tree = ComponentList::new(WindowRoot::new(root));
    tree.on_event(&Event::WindowResized {
        width: width as u32,
        height: height as u32,
    });
    tree
}

#[test]
fn a_stretched_line_is_centred_in_a_taller_box_by_whole_rows() {
    let text = label("Setup", LayoutStyle::new().width(160.0).height(80.0));
    let tree = drawn(160.0, 80.0, box_item(text));
    let tops: Vec<f32> = lines(&tree).iter().map(|line| line.y).collect();
    assert_eq!(tops, vec![32.0]);
}

#[test]
fn a_row_of_labels_shares_one_row_whatever_letters_they_have() {
    let labels = ["Setup", "Simulation", "HOME", "xo", "gyp"]
        .into_iter()
        .map(|content| box_item(label(content, LayoutStyle::new())))
        .collect();
    let row = Container::new(
        LayoutStyle::new()
            .flex_row()
            .gap(8.0)
            .width(480.0)
            .height(48.0),
        labels,
    )
    .unwrap();
    let tree = drawn(480.0, 48.0, box_item(row));
    let tops: Vec<f32> = lines(&tree).iter().map(|line| line.y).collect();
    assert_eq!(
        tops,
        vec![ROW; 5],
        "every label on the middle of three rows"
    );
}

#[test]
fn several_lines_are_centred_as_one_block_of_rows() {
    let text = label(
        "the quick brown fox",
        LayoutStyle::new().width(80.0).height(64.0),
    );
    let tree = drawn(80.0, 64.0, box_item(text));
    let lines = lines(&tree);
    assert_eq!(lines.len(), 1, "one paragraph");
    assert_eq!(
        (lines[0].y, lines[0].height),
        (ROW, 2.0 * ROW),
        "two rows in the middle of four"
    );
}

#[test]
fn nothing_is_drawn_outside_the_leaf() {
    for rows in 1..=4 {
        let height = rows as f32 * ROW;
        let text = label("gyp", LayoutStyle::new().width(80.0).height(height));
        let tree = drawn(80.0, height, box_item(text));
        let line = lines(&tree)[0];
        assert!(
            line.y >= 0.0 && line.y + line.height <= height,
            "in {rows} rows the line takes {line:?}"
        );
    }
}

#[test]
fn an_odd_spare_row_goes_below_the_line() {
    let text = label("Setup", LayoutStyle::new().width(80.0).height(2.0 * ROW));
    let tree = drawn(80.0, 2.0 * ROW, box_item(text));
    assert_eq!(
        lines(&tree)[0].y,
        0.0,
        "the top row of two, the spare one below"
    );
}

#[test]
fn a_field_and_a_label_beside_it_share_one_row() {
    on_cells();
    let field = Input::new(
        reactive_core::signal("Ada".to_string()),
        LayoutStyle::new().width(160.0),
        || TextStyle::new(14.0, Color::WHITE),
    )
    .unwrap();
    let row = Container::new(
        LayoutStyle::new()
            .flex_row()
            .gap(8.0)
            .width(320.0)
            .height(3.0 * ROW),
        vec![box_item(label("Name", LayoutStyle::new())), box_item(field)],
    )
    .unwrap();
    let tree = drawn(320.0, 3.0 * ROW, box_item(row));
    let tops: Vec<f32> = lines(&tree).iter().map(|line| line.y).collect();
    assert_eq!(
        tops,
        vec![ROW, ROW],
        "the label and the field on the middle row"
    );
}
