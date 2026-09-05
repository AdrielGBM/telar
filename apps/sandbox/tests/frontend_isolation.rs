//! The terminal backend teaches the layout what a cell costs. Every other frontend must not notice.
//!
//! The mechanism is a grid that defaults to [`LayoutGrid::UNIT`], where every snap is the identity — but a default is only a promise until something checks it. These are the two halves of that promise for the one value that could not be settled by snapping alone: what a single line of text reserves.

use renderer_tui::{CellMetrics, CellSize};

/// A raster surface has room between lines to spend, and how much is a design decision this app made long before any of this existed. Changing it would be a visible regression on every desktop and browser build, for a reason that belongs to neither.
#[test]
fn a_surface_without_a_grid_keeps_the_leading_it_always_had() {
    telar::set_layout_grid(telar::LayoutGrid::UNIT);
    renderer_core::set_text_metrics(renderer_text::ShaperMetrics);
    for size in [11.0f32, 14.0, 32.0] {
        assert_eq!(
            ui_core::single_line_box(size),
            size * ui_core::SINGLE_LINE_LEADING,
            "a unit grid changed what a {size}px line reserves"
        );
    }
}

/// A terminal draws one glyph per cell whatever size the text claims, so there is no decision to make: a line is a cell. A 32px title given the leading above would reserve three rows to paint one, which is what used to push everything under a heading off the grid.
#[test]
fn a_terminal_reserves_one_cell_whatever_the_font_size() {
    let cell = CellSize::default();
    renderer_core::set_text_metrics(CellMetrics::new(cell));
    telar::set_layout_grid(telar::LayoutGrid::new(cell.width, cell.height));
    for size in [11.0f32, 14.0, 32.0] {
        assert_eq!(
            ui_core::single_line_box(size),
            cell.height,
            "a {size}px line took more than its row"
        );
    }
}
