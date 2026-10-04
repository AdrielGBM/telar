use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{
    BlendMode, Border, BorderRadius, Color, Paint, RectStyle, ShapeStyle, TextStyle,
};

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
                blend: BlendMode::Normal,
                mask: renderer_core::LayerMask::None,
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

fn line_cmd(stroke: renderer_core::Stroke) -> DrawCommand {
    DrawCommand::Line {
        p1: geometry_core::Point::new(0.0, 0.0),
        p2: geometry_core::Point::new(8.0 * 3.0, 0.0),
        style: Arc::new(stroke),
    }
}

#[test]
fn a_dashed_stroke_is_drawn_in_dashed_line_characters() {
    let solid = renderer_core::Stroke::new(Color::WHITE, 1.0);
    for (stroke, expected) in [
        (solid, "────  "),
        (solid.with_dash(&[4.0, 4.0], 0.0), "╌╌╌╌  "),
        (solid.with_dash(&[1.0, 4.0], 0.0), "┈┈┈┈  "),
    ] {
        let mut buf = grid(6, 1);
        paint(&mut buf, &[line_cmd(stroke)]);
        assert_eq!(row(&buf, 0), expected);
    }
}

#[test]
fn a_dashed_path_runs_vertically_in_dashed_line_characters() {
    let mut buf = grid(1, 3);
    let path = renderer_core::PathData::new()
        .move_to(geometry_core::Point::new(0.0, 0.0))
        .line_to(geometry_core::Point::new(0.0, 16.0 * 2.0));
    paint(
        &mut buf,
        &[DrawCommand::Path {
            data: Arc::new(path),
            style: Arc::new(renderer_core::PathStyle::default().with_stroke(
                renderer_core::Stroke::new(Color::WHITE, 1.0).with_dash(&[1.0, 4.0], 0.0),
            )),
        }],
    );
    assert_eq!((0..3).map(|r| row(&buf, r)).collect::<String>(), "┊┊┊");
}

fn element_linking(id: u64, destination: renderer_core::Destination) -> DrawCommand {
    DrawCommand::PushElement {
        element: Arc::new(renderer_core::Element::new(
            renderer_core::ElementId(id),
            renderer_core::Semantics::group().linking_to(destination),
            "",
            Rect::default(),
        )),
    }
}

#[test]
fn the_glyphs_of_an_external_link_are_written_as_an_osc_8_hyperlink() {
    let mut buf = grid(12, 1);
    let uri = renderer_core::Destination::external("https://example.com").unwrap();
    paint(
        &mut buf,
        &[
            text_cmd("go", Rect::new(0.0, 0.0, 16.0, 16.0)),
            element_linking(1, uri),
            text_cmd("ab", Rect::new(3.0 * 8.0, 0.0, 16.0, 16.0)),
            DrawCommand::PopElement,
        ],
    );
    let mut out = Vec::new();
    buf.diff_into(&grid(0, 0), crate::ColorDepth::TrueColor, &mut out);
    let out = String::from_utf8(out).unwrap();
    let opened = out
        .find("\x1b]8;;https://example.com\x1b\\")
        .expect("the link is opened");
    let closed = out.rfind("\x1b]8;;\x1b\\").expect("and closed");
    let linked = &out[opened..closed];
    assert!(linked.contains('a') && linked.contains('b'));
    assert!(!linked.contains('g'), "text outside the link is not in it");
}

#[test]
fn a_route_link_has_nothing_a_terminal_can_open() {
    let mut buf = grid(4, 1);
    let route = renderer_core::Destination::Route(Default::default());
    paint(
        &mut buf,
        &[
            element_linking(1, route),
            text_cmd("ab", Rect::new(0.0, 0.0, 16.0, 16.0)),
            DrawCommand::PopElement,
        ],
    );
    let mut out = Vec::new();
    buf.diff_into(&grid(0, 0), crate::ColorDepth::TrueColor, &mut out);
    assert!(!String::from_utf8(out).unwrap().contains("\x1b]8"));
}

/// A terminal has no alpha to show content through, so a mask draws its content whole and its source not at all.
#[test]
fn a_mask_source_is_never_drawn_and_its_content_is() {
    let layer = |mask| DrawCommand::PushLayer {
        opacity: 1.0,
        backdrop_blur: 0.0,
        blend: BlendMode::Normal,
        mask,
    };
    let mut buf = grid(12, 2);
    paint(
        &mut buf,
        &[
            layer(renderer_core::LayerMask::Source),
            text_cmd("SOURCE", Rect::new(0.0, 0.0, 96.0, 16.0)),
            DrawCommand::PopLayer,
            layer(renderer_core::LayerMask::Apply),
            text_cmd("content", Rect::new(0.0, 16.0, 96.0, 16.0)),
            DrawCommand::PopLayer,
        ],
    );
    assert!(!row(&buf, 0).contains("SOURCE"), "{:?}", row(&buf, 0));
    assert!(row(&buf, 1).starts_with("content"), "{:?}", row(&buf, 1));
}

fn styled_text_cmd(text: &str, rect: Rect, style: TextStyle) -> DrawCommand {
    DrawCommand::Text {
        text: text.into(),
        spans: None,
        rect,
        style: Arc::new(style),
    }
}

#[test]
fn a_text_case_is_drawn_in_the_cells_and_measured_the_same() {
    let mut buf = grid(12, 1);
    let style = TextStyle::new(14.0, Paint::Solid(Color::WHITE))
        .with_text_case(renderer_core::TextCase::Upper);
    paint(
        &mut buf,
        &[styled_text_cmd(
            "straße",
            Rect::new(0.0, 0.0, 8.0 * 12.0, 16.0),
            style.clone(),
        )],
    );
    assert_eq!(row(&buf, 0), "STRASSE     ");
    let metrics = crate::CellMetrics::new(CellSize::default());
    assert_eq!(
        renderer_core::TextMetrics::measure(&metrics, "straße", None, 1000.0, &style).0,
        8.0 * 7.0,
        "measured as the seven cells it draws"
    );
}

#[test]
fn an_underline_marks_its_cells_and_nothing_else() {
    let mut buf = grid(12, 2);
    let underlined = TextStyle::new(14.0, Paint::Solid(Color::WHITE))
        .with_underline(true)
        .with_underline_offset(4.0);
    paint(
        &mut buf,
        &[
            styled_text_cmd("link", Rect::new(0.0, 0.0, 8.0 * 12.0, 16.0), underlined),
            text_cmd("text", Rect::new(0.0, 16.0, 8.0 * 12.0, 16.0)),
        ],
    );
    assert!(buf.get(0, 0).unwrap().attrs.contains(Attrs::UNDERLINE));
    assert!(buf.get(3, 0).unwrap().attrs.contains(Attrs::UNDERLINE));
    assert!(!buf.get(0, 1).unwrap().attrs.contains(Attrs::UNDERLINE));
}
