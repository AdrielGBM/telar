//! On a surface that quantises, a box has to have a size — not a size that depends on where it is.
//!
//! The terminal maps an edge with `round(v / step)` and rounds a box's two edges independently, which is what makes two boxes sharing an edge share a cell instead of leaving a seam. The cost is that the cells a box covers are `round((y + h) / step) - round(y / step)`: a function of `y` as well as `h`. A box 2.3 cells tall is drawn 2 cells tall at one scroll offset and 3 at the next, and two identical boxes that landed at different heights disagree while nothing is moving at all.
//!
//! Snapping what layout is *told* removes the dependency rather than papering over it: with `h` a whole number of steps the expression collapses to `h / step` for every `y`. This asserts that on the real tree — every box a whole number of cells, at a position that is one too, so its size is the same at every scroll offset.

use renderer_tui::{CellMetrics, CellSize};
use telar::{App, DrawCommand, Event};

/// A surface the terminal could really report: `platform_tui::TuiWindow` sizes itself as `cols x cell_width` by `rows x cell_height`, so a whole number of cells is a precondition the frontend guarantees rather than something layout has to defend against. 150x56 cells.
const SURFACE: (u32, u32) = (150 * 8, 56 * 16);

/// The scroll offsets a wheel actually produces. A notch is 20 logical pixels and a cell is 16, so successive notches land at every fractional cell position there is — which is exactly what used to make the size flicker.
const OFFSETS: [f32; 5] = [0.0, 20.0, 40.0, 60.0, 80.0];

fn boxes_of(tree: &telar::ComponentList) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    telar::for_each_with_matrix(&tree.commands(), |c, m| {
        if let DrawCommand::Rect { rect, .. } = c
            && rect.height > 0.0
        {
            out.push((m[5] + rect.y, rect.height));
        }
    });
    out
}

#[test]
fn every_box_keeps_its_height_at_every_scroll_offset() {
    let cell = CellSize::default();
    renderer_core::set_text_metrics(CellMetrics::new(cell));
    telar::set_layout_grid(telar::LayoutGrid::new(cell.width, cell.height));
    telar::set_theme(sandbox::core::theme::SandboxTheme::modern());

    let mut tree = telar::ComponentList::new(sandbox::core::app::SandboxRoot.root());
    tree.on_event(&Event::WindowResized {
        width: SURFACE.0,
        height: SURFACE.1,
    });

    // The painter's own mapping, not a copy of it: a test that reimplements the rounding cannot catch the rounding being wrong.
    let row_at = |v: f32| cell.row_at(v);
    let boxes = boxes_of(&tree);
    assert!(!boxes.is_empty(), "the tree drew nothing to check");

    for (y, h) in boxes {
        let rows: Vec<i32> = OFFSETS
            .iter()
            .map(|off| row_at(y - off + h) - row_at(y - off))
            .collect();
        assert!(
            rows.iter().all(|r| *r == rows[0]),
            "a box {h}px tall at y={y} covers {rows:?} rows at offsets {OFFSETS:?} — its height \
             depends on where it is, so it changes size as the page scrolls"
        );
    }
}

/// The other half, and the one that makes the first hold for *any* offset rather than the five sampled: a length the grid is installed for comes back a whole number of cells.
#[test]
fn layout_is_told_only_whole_cells() {
    let cell = CellSize::default();
    renderer_core::set_text_metrics(CellMetrics::new(cell));
    telar::set_layout_grid(telar::LayoutGrid::new(cell.width, cell.height));
    telar::set_theme(sandbox::core::theme::SandboxTheme::modern());

    let mut tree = telar::ComponentList::new(sandbox::core::app::SandboxRoot.root());
    tree.on_event(&Event::WindowResized {
        width: SURFACE.0,
        height: SURFACE.1,
    });

    for (y, h) in boxes_of(&tree) {
        assert_eq!(h % cell.height, 0.0, "a box {h}px tall is not whole cells");
        assert_eq!(
            y % cell.height,
            0.0,
            "a box at y={y} does not start on a cell"
        );
    }
}
