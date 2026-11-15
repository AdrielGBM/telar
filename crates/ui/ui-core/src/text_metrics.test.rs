use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::{CellSize, Size};
use layout_core::LayoutStyle;
use reactive_core::effect;
use renderer_core::{Color, FontAsset, FontFamily, FontWeight, TextStyle};
use renderer_text::ShaperMetrics;
use renderer_tui::CellMetrics;

use super::*;
use crate::{Surface, SurfaceCanvas, Text};

const TEST_FACE: &[u8] = include_bytes!("../../../renderer/renderer-text/test-fonts/TelarTest.ttf");

const WORDS: &str = "Wide Words";

fn style() -> TextStyle {
    TextStyle::new(14.0, Color::BLACK)
}

fn width_in(surface: &Surface) -> f32 {
    surface.with_entered(|| measure_text(WORDS, None, 1.0e6, &style()).0)
}

/// What `opening_fit`-style code does by hand: measure a line in a face the page is still fetching. Measured in the fallback first, it has to measure again when the face lands, or it keeps the fallback's width for good.
#[test]
fn a_face_that_lands_runs_whatever_measured_text_again() {
    crate::context::reset_layout_runtime();
    let style = TextStyle::new(40.0, Color::BLACK).with_font_family(FontFamily::stack([
        FontFamily::Named("Telar Arriving Face".into()),
        FontFamily::Monospace,
    ]));
    let widths = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&widths);
    effect(move || {
        use_text_metrics_generation();
        seen.borrow_mut()
            .push(measure_text("Wide Words", None, 1.0e6, &style).0);
    });
    assert_eq!(widths.borrow().len(), 1);

    let generation = text_metrics_generation();
    renderer_text::fonts::add_faces(vec![
        FontAsset::embedded(TEST_FACE)
            .named("Telar Arriving Face")
            .with_weight(FontWeight::range(100, 900).unwrap()),
    ]);
    assert!(text_metrics_generation() > generation);
    assert_eq!(
        widths.borrow().len(),
        1,
        "the generation moves on the next frame, not inside whatever added the face"
    );

    crate::context::relayout_if_dirty();
    let widths = widths.borrow();
    assert_eq!(
        widths.len(),
        2,
        "the frame after a face lands runs the effect again"
    );
    assert_ne!(
        widths[0], widths[1],
        "and it measures in the face that landed rather than the fallback"
    );
}

#[test]
fn two_surfaces_measure_the_same_text_with_their_own_measurers() {
    let cell = CellSize::default();
    let glyphs = Surface::new();
    let cells = Surface::new();
    glyphs.with_entered(|| open_surface_text_metrics(Arc::new(ShaperMetrics)));
    cells.with_entered(|| open_surface_text_metrics(Arc::new(CellMetrics::new(cell))));

    let in_cells = width_in(&cells);
    let in_glyphs = width_in(&glyphs);
    assert_eq!(
        in_cells,
        WORDS.len() as f32 * cell.width(),
        "a terminal's surface sizes a string in whole cells"
    );
    assert_ne!(
        in_glyphs, in_cells,
        "a raster surface sizes the same string by its shaped advance"
    );
    assert_eq!(
        width_in(&cells),
        in_cells,
        "measuring on one surface leaves the other's measurer where it was"
    );
}

#[test]
fn a_measurer_opened_over_another_moves_the_generation() {
    let surface = Surface::new();
    surface.with_entered(|| {
        open_surface_text_metrics(Arc::new(ShaperMetrics));
        let before = text_metrics_generation();
        open_surface_text_metrics(Arc::new(CellMetrics::new(CellSize::default())));
        assert_ne!(
            text_metrics_generation(),
            before,
            "text measured by the old measurer is measured again by the new one"
        );
    });
}

#[test]
fn a_canvas_lays_its_text_out_with_the_measurer_of_the_surface_it_is_built_in() {
    let cell = CellSize::default();
    let host = Surface::new();
    host.with_entered(|| open_surface_text_metrics(Arc::new(CellMetrics::new(cell))));
    let canvas = host.with_entered(|| {
        SurfaceCanvas::new(Size::new(400.0, 100.0), || {
            Ok(Box::new(Text::new(
                || WORDS.to_string(),
                LayoutStyle::new(),
                style,
            )?))
        })
        .expect("the canvas builds")
    });
    assert_eq!(
        canvas.with_entered(|| measure_text(WORDS, None, 1.0e6, &style()).0),
        WORDS.len() as f32 * cell.width(),
    );
}
