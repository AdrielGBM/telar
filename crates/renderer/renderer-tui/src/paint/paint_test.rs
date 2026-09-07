use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{Border, BorderRadius, Color, Paint, RectStyle, ShapeStyle, TextStyle};

use super::*;

fn grid(cols: u16, rows: u16) -> CellBuffer {
    CellBuffer::new(cols, rows, Rgb::BLACK)
}

/// The characters of one row, so an assertion reads like the screen does.
fn row(buf: &CellBuffer, row: u16) -> String {
    (0..buf.cols())
        .filter_map(|c| buf.get(c, row))
        .filter(|c| !c.attrs.contains(Attrs::WIDE_TAIL))
        .map(|c| c.glyph.as_str())
        .collect()
}

fn paint(buf: &mut CellBuffer, commands: &[DrawCommand]) {
    Painter::new(buf, CellSize::default(), crate::ColorDepth::TrueColor).paint(commands);
}

fn rect_cmd(rect: Rect, style: RectStyle) -> DrawCommand {
    DrawCommand::Rect {
        rect,
        style: Arc::new(style),
    }
}

fn text_cmd(text: &str, rect: Rect) -> DrawCommand {
    DrawCommand::Text {
        text: text.into(),
        spans: None,
        rect,
        style: Arc::new(TextStyle::new(14.0, Paint::Solid(Color::WHITE))),
    }
}

#[test]
fn a_fill_colours_every_cell_it_covers() {
    let mut buf = grid(10, 3);
    paint(
        &mut buf,
        &[rect_cmd(
            Rect::new(0.0, 0.0, 8.0 * 4.0, 16.0 * 2.0),
            RectStyle::default().with_fill(Color::RED),
        )],
    );
    assert_eq!(buf.get(3, 1).unwrap().bg, Rgb { r: 255, g: 0, b: 0 });
    assert_eq!(
        buf.get(4, 1).unwrap().bg,
        Rgb::BLACK,
        "one column past the edge"
    );
    assert_eq!(
        buf.get(0, 2).unwrap().bg,
        Rgb::BLACK,
        "one row past the edge"
    );
}

#[test]
fn a_border_draws_a_closed_frame() {
    let mut buf = grid(10, 4);
    paint(
        &mut buf,
        &[rect_cmd(
            Rect::new(0.0, 0.0, 8.0 * 5.0, 16.0 * 3.0),
            RectStyle::default().with_border(Border::uniform(Paint::Solid(Color::WHITE), 1.0)),
        )],
    );
    assert_eq!(row(&buf, 0), "┌───┐     ");
    assert_eq!(row(&buf, 1), "│   │     ");
    assert_eq!(row(&buf, 2), "└───┘     ");
}

#[test]
fn a_rounded_border_uses_rounded_corners() {
    let mut buf = grid(6, 3);
    paint(
        &mut buf,
        &[rect_cmd(
            Rect::new(0.0, 0.0, 8.0 * 4.0, 16.0 * 3.0),
            RectStyle::default()
                .with_border(Border::uniform(Paint::Solid(Color::WHITE), 1.0))
                .with_radius(BorderRadius::all(8.0)),
        )],
    );
    assert_eq!(row(&buf, 0), "╭──╮  ");
    assert_eq!(row(&buf, 2), "╰──╯  ");
}

#[test]
fn a_thick_border_uses_the_heavy_set() {
    let mut buf = grid(6, 3);
    paint(
        &mut buf,
        &[rect_cmd(
            Rect::new(0.0, 0.0, 8.0 * 4.0, 16.0 * 3.0),
            RectStyle::default().with_border(Border::uniform(Paint::Solid(Color::WHITE), 6.0)),
        )],
    );
    assert_eq!(row(&buf, 0), "┏━━┓  ");
}

#[test]
fn a_border_on_one_side_only_draws_that_side() {
    let mut buf = grid(6, 3);
    paint(
        &mut buf,
        &[rect_cmd(
            Rect::new(0.0, 0.0, 8.0 * 4.0, 16.0 * 3.0),
            RectStyle::default().with_border(Border::per_side(
                Paint::Solid(Color::WHITE),
                0.0,
                0.0,
                1.0,
                0.0,
            )),
        )],
    );
    assert_eq!(row(&buf, 0), "      ");
    assert_eq!(row(&buf, 2), "────  ");
}

#[test]
fn text_starts_where_its_box_does() {
    let mut buf = grid(12, 2);
    paint(
        &mut buf,
        &[text_cmd(
            "hola",
            Rect::new(8.0 * 2.0, 16.0, 8.0 * 4.0, 16.0),
        )],
    );
    assert_eq!(row(&buf, 1), "  hola      ");
}

#[test]
fn centred_text_is_centred_in_its_box() {
    let mut buf = grid(12, 1);
    let mut style = TextStyle::new(14.0, Paint::Solid(Color::WHITE));
    style.text_align = renderer_core::TextAlign::Center;
    paint(
        &mut buf,
        &[DrawCommand::Text {
            text: "ab".into(),
            spans: None,
            rect: Rect::new(0.0, 0.0, 8.0 * 8.0, 16.0),
            style: Arc::new(style),
        }],
    );
    assert_eq!(row(&buf, 0), "   ab       ");
}

#[test]
fn a_clip_cuts_the_paragraph() {
    let mut buf = grid(12, 1);
    paint(
        &mut buf,
        &[
            DrawCommand::PushClip {
                rect: Rect::new(0.0, 0.0, 8.0 * 2.0, 16.0),
                radius: BorderRadius::zero(),
            },
            text_cmd("abcdef", Rect::new(0.0, 0.0, 8.0 * 6.0, 16.0)),
            DrawCommand::PopClip,
        ],
    );
    assert_eq!(row(&buf, 0), "ab          ");
}

#[test]
fn a_matrix_moves_what_is_drawn_under_it() {
    let mut buf = grid(12, 2);
    paint(
        &mut buf,
        &[
            DrawCommand::PushMatrix {
                matrix: [1.0, 0.0, 0.0, 1.0, 8.0 * 3.0, 16.0],
            },
            text_cmd("hi", Rect::new(0.0, 0.0, 8.0 * 2.0, 16.0)),
            DrawCommand::PopMatrix,
        ],
    );
    assert_eq!(row(&buf, 1), "   hi       ");
}

#[test]
fn a_layer_fades_what_it_holds() {
    let mut buf = grid(4, 1);
    paint(
        &mut buf,
        &[
            DrawCommand::PushLayer {
                opacity: 0.5,
                backdrop_blur: 0.0,
            },
            rect_cmd(
                Rect::new(0.0, 0.0, 8.0 * 4.0, 16.0),
                RectStyle::default().with_fill(Color::WHITE),
            ),
            DrawCommand::PopLayer,
        ],
    );
    let bg = buf.get(0, 0).unwrap().bg;
    assert!(bg.r.abs_diff(128) <= 1, "got {bg:?}");
}

#[test]
fn a_caret_thinner_than_a_cell_is_drawn_as_a_bar() {
    let mut buf = grid(8, 1);
    paint(
        &mut buf,
        &[rect_cmd(
            Rect::new(8.0 * 3.0, 0.0, 2.0, 16.0),
            RectStyle::default().with_fill(Color::WHITE),
        )],
    );
    assert_eq!(row(&buf, 0), "   │    ");
}

#[test]
fn a_hairline_divider_is_drawn_as_a_rule() {
    let mut buf = grid(6, 2);
    paint(
        &mut buf,
        &[rect_cmd(
            Rect::new(0.0, 16.0, 8.0 * 4.0, 1.0),
            RectStyle::default().with_fill(Color::WHITE),
        )],
    );
    assert_eq!(row(&buf, 1), "────  ");
}

#[test]
fn a_zero_sized_rect_draws_nothing() {
    let mut buf = grid(4, 1);
    paint(
        &mut buf,
        &[rect_cmd(
            Rect::new(0.0, 0.0, 0.0, 0.0),
            RectStyle::default().with_fill(Color::WHITE),
        )],
    );
    assert_eq!(row(&buf, 0), "    ");
}

#[test]
fn a_filled_path_covers_its_interior() {
    let mut buf = grid(8, 4);
    let path = renderer_core::PathData::new()
        .move_to(geometry_core::Point::new(0.0, 0.0))
        .line_to(geometry_core::Point::new(8.0 * 4.0, 0.0))
        .line_to(geometry_core::Point::new(8.0 * 4.0, 16.0 * 2.0))
        .line_to(geometry_core::Point::new(0.0, 16.0 * 2.0))
        .close();
    paint(
        &mut buf,
        &[DrawCommand::Path {
            data: Arc::new(path),
            style: Arc::new(renderer_core::PathStyle::default().with_fill(Color::GREEN)),
        }],
    );
    assert_eq!(buf.get(1, 1).unwrap().bg, Rgb { r: 0, g: 255, b: 0 });
    assert_eq!(buf.get(5, 1).unwrap().bg, Rgb::BLACK, "outside the path");
}
