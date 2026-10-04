//! The arrival margin a page takes from the layers fixed over it, when it declares none: the strips along its top and bottom that their bars cover.

use geometry_core::{Insets, Rect, Size};

use crate::context::{track_layout, use_surface_size};
use crate::input_region::tracks_hidden;
use crate::scroll_viewports::fixed_layer_boxes;

/// How near an edge of the surface a box has to be to stand against it, so that layout rounding does not decide it.
const AGAINST: f32 = 0.5;

/// The strips along the top and bottom of the surface that the shown layers' boxes cover, read reactively: whoever reads it runs again when a layer comes or goes, is hidden or shown, or one of its boxes moves.
pub(crate) fn covered_by_layers() -> (f32, f32) {
    let surface = use_surface_size();
    let boxes: Vec<Rect> = fixed_layer_boxes()
        .into_iter()
        .filter(|&node| !tracks_hidden(node))
        .filter_map(|node| Some(track_layout(node)?.get()))
        .collect();
    covered_edges(surface, &boxes)
}

/// How far down from the top and up from the bottom of `surface` the bars among `boxes` reach. A bar is a box that stands against one of the two edges and not the other: a box as tall as the surface is a side panel, which covers no edge a page arrives at.
pub(crate) fn covered_edges(surface: Size, boxes: &[Rect]) -> (f32, f32) {
    boxes
        .iter()
        .filter(|rect| rect.width > 0.0 && rect.height > 0.0)
        .fold((0.0f32, 0.0f32), |(top, bottom), rect| {
            let at_top = rect.y <= AGAINST;
            let at_bottom = rect.y + rect.height >= surface.height - AGAINST;
            match (at_top, at_bottom) {
                (true, false) => (top.max(rect.y + rect.height), bottom),
                (false, true) => (top, bottom.max(surface.height - rect.y)),
                _ => (top, bottom),
            }
        })
}

/// The part of `viewport`, a rect on `surface`, that the strips `(top, bottom)` along the surface's edges cover: a page kept clear of a notch already starts below some of a bar.
pub(crate) fn arrival_within((top, bottom): (f32, f32), surface: Size, viewport: Rect) -> Insets {
    let span = viewport.height.max(0.0);
    let from_bottom = surface.height - (viewport.y + viewport.height);
    Insets::new(
        (top - viewport.y).clamp(0.0, span),
        0.0,
        (bottom - from_bottom).clamp(0.0, span),
        0.0,
    )
}

#[cfg(test)]
#[path = "arrival_margin_test.rs"]
mod tests;
