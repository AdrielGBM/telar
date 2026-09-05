//! What a cell can express that a pixel cannot, and the two places the difference used to be ignored.

use std::sync::Arc;

use geometry_core::Rect;
use renderer_core::{BorderRadius, Color, DrawCommand, RectStyle, ShapeStyle, TextStyle};
use telar_renderer_tui::{CellBuffer, CellSize, Painter, Rgb};

const FILL: Color = Color {
    r: 60.0 / 255.0,
    g: 60.0 / 255.0,
    b: 200.0 / 255.0,
    a: 1.0,
};

fn draw(cols: u16, rows: u16, cmds: &[DrawCommand]) -> CellBuffer {
    let mut buf = CellBuffer::new(cols, rows, Rgb::BLACK);
    Painter::new(
        &mut buf,
        CellSize::default(),
        telar_renderer_tui::ColorDepth::TrueColor,
    )
    .paint(cmds);
    buf
}

fn row_text(buf: &CellBuffer, row: u16, cols: u16) -> String {
    (0..cols)
        .filter_map(|c| buf.get(c, row))
        .map(|c| c.glyph.as_str())
        .collect()
}

fn text(s: &str, y: f32) -> DrawCommand {
    DrawCommand::Text {
        spans: None,
        text: Arc::from(s),
        rect: Rect::new(0.0, y, 160.0, 16.0),
        style: Arc::new(TextStyle::new(14.0, Color::WHITE)),
    }
}

fn panel(w: f32, h: f32, radius: f32, alpha: f32) -> DrawCommand {
    DrawCommand::Rect {
        rect: Rect::new(0.0, 0.0, w, h),
        style: Arc::new(
            RectStyle::default()
                .with_fill(FILL.with_alpha(alpha))
                .with_radius(BorderRadius::all(radius)),
        ),
    }
}

/// A cell holds one glyph and has no partial coverage, so an opaque fill has to take the glyph with it. Without this a modal, a menu and a tooltip were all legible *through*, and the page behind them read as though it were still in front.
#[test]
fn an_opaque_fill_takes_the_glyph_under_it() {
    let buf = draw(20, 1, &[text("HELLO", 0.0), panel(160.0, 16.0, 0.0, 1.0)]);
    assert!(
        !row_text(&buf, 0, 20).contains("HELLO"),
        "the text survived under an opaque panel"
    );
}

/// The other side of the same rule, and the reason it is not simply "a fill clears": a scrim is half alpha by definition, and it exists to dim the page rather than erase it.
#[test]
fn a_half_alpha_wash_dims_the_page_instead_of_erasing_it() {
    let buf = draw(20, 1, &[text("HELLO", 0.0), panel(160.0, 16.0, 0.0, 0.5)]);
    assert!(
        row_text(&buf, 0, 20).contains("HELLO"),
        "a scrim erased the page it was meant to dim"
    );
}

/// A radius smaller than a cell has nowhere to show: the corner cell's centre is still inside the shape, so a lightly rounded box fills exactly as a square one does.
#[test]
fn a_sub_cell_radius_fills_square() {
    let filled = |radius: f32| {
        let buf = draw(28, 10, &[panel(224.0, 160.0, radius, 1.0)]);
        (0..10u16)
            .flat_map(|r| (0..28u16).map(move |c| (c, r)))
            .filter(|(c, r)| {
                buf.get(*c, *r).map(|x| x.bg)
                    == Some(Rgb {
                        r: 60,
                        g: 60,
                        b: 200,
                    })
            })
            .count()
    };
    assert_eq!(filled(0.0), 280, "a square box should fill every cell");
    assert_eq!(filled(8.0), 280, "half a cell of radius rounds nothing off");
}

/// A radius of several cells does, and the corners have to come off the *fill* — the border characters alone left a rectangle with rounded glyphs drawn on its corners.
#[test]
fn a_large_radius_rounds_the_fill_itself() {
    let buf = draw(28, 10, &[panel(224.0, 160.0, 80.0, 1.0)]);
    let is_fill = |c: u16, r: u16| {
        buf.get(c, r).map(|x| x.bg)
            == Some(Rgb {
                r: 60,
                g: 60,
                b: 200,
            })
    };
    assert!(
        !is_fill(0, 0),
        "the top-left corner cell should be outside a 5-cell radius"
    );
    assert!(!is_fill(27, 9), "and so should the bottom-right");
    assert!(is_fill(14, 0), "the middle of the top edge stays inside");
    assert!(is_fill(0, 5), "and so does the middle of the left edge");
}

fn bordered(w: f32, h: f32, radius: f32, fill: bool) -> DrawCommand {
    let mut style = RectStyle::default()
        .with_border(renderer_core::Border::uniform(Color::WHITE, 1.0))
        .with_radius(BorderRadius::all(radius));
    if fill {
        style = style.with_fill(FILL);
    }
    DrawCommand::Rect {
        rect: Rect::new(0.0, 0.0, w, h),
        style: Arc::new(style),
    }
}

/// A box one row tall has no row for a rule that is not also its content's row, so a rule there is a frame struck through the thing it frames — `+--Outline--+` with the label sitting in the hyphens. It keeps its uprights instead.
#[test]
fn a_single_row_box_keeps_its_uprights_and_drops_its_rules() {
    let buf = draw(
        14,
        1,
        &[bordered(112.0, 16.0, 0.0, false), text("Outline", 0.0)],
    );
    let row = row_text(&buf, 0, 14);
    assert!(
        !row.contains('\u{2500}'),
        "a one-row box drew a horizontal rule: {row:?}"
    );
    assert!(
        row.contains('\u{2502}'),
        "and it should still have its sides: {row:?}"
    );
}

/// A box two rows tall or more has somewhere to put them.
#[test]
fn a_taller_box_still_draws_its_rules() {
    let buf = draw(14, 3, &[bordered(112.0, 48.0, 0.0, false)]);
    assert!(
        row_text(&buf, 0, 14).contains('\u{2500}'),
        "a three-row box should be framed top and bottom"
    );
}

/// A checkbox, a radio and a switch's knob are each a box a cell or two across. Framed, the border characters *are* the whole control and come out as `+|`; a terminal has a character for the thing itself.
#[test]
fn a_box_too_small_to_frame_is_drawn_as_a_mark() {
    let square = draw(2, 1, &[bordered(16.0, 16.0, 2.0, false)]);
    assert_eq!(square.get(0, 0).unwrap().glyph.as_str(), "\u{25a1}");

    // A radius of half the shorter side is the pill test every rasteriser uses, and it is what separates a radio from a checkbox without the painter being told which is which.
    let circle = draw(2, 1, &[bordered(16.0, 16.0, 8.0, false)]);
    assert_eq!(circle.get(0, 0).unwrap().glyph.as_str(), "\u{25cb}");
}

/// The mark is always the outline: "on" reads by the colour behind it, exactly as it does on a raster backend, so a fill that merely gives the control a surface is not mistaken for one that means checked.
#[test]
fn a_marks_state_is_its_background_not_its_glyph() {
    let on = draw(2, 1, &[bordered(16.0, 16.0, 8.0, true)]);
    let cell = on.get(0, 0).unwrap();
    assert_eq!(
        cell.glyph.as_str(),
        "\u{25cb}",
        "a filled control keeps the outline glyph"
    );
    assert_eq!(
        cell.bg,
        Rgb {
            r: 60,
            g: 60,
            b: 200
        },
        "and shows its state behind it"
    );
}
