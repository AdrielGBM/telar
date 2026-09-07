//! The two box properties whose edges can be named by role rather than by side, resolved against the active writing direction.
//!
//! `start`/`end` mean here what `padding_start` means in layout: the edge the text comes from, and the one it runs towards. Layout resolves its own in a pass it already had; paint has no such pass, so the flip happens inside the style closure the renderer re-runs — which is also what makes a live LTR/RTL switch repaint instead of needing the tree rebuilt.

use renderer_core::BorderRadius;

use crate::context::use_direction;

/// Per-side border widths where `start`/`end`, when given, land on left or right according to the writing direction.
///
/// A logical side overrides the physical one it lands on: an author who wrote both named this edge twice, and the name that describes its *role* is the one that was talking about the current layout.
pub fn logical_border_widths(
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
    start: Option<f32>,
    end: Option<f32>,
) -> [f32; 4] {
    let (mut left, mut right) = (left, right);
    let rtl = use_direction().is_rtl();
    if let Some(w) = start {
        if rtl { right = w } else { left = w }
    }
    if let Some(w) = end {
        if rtl { left = w } else { right = w }
    }
    [top, right, bottom, left]
}

/// Corner radii where `start`/`end`, when given, round the two corners on that edge.
///
/// A side rather than a corner, because that is the shape the property is actually for: a panel tucked against the rail is rounded on the two corners facing away from it, and under RTL it has to tuck against the other rail without the author writing the layout twice.
pub fn logical_border_radius(
    top_left: f32,
    top_right: f32,
    bottom_right: f32,
    bottom_left: f32,
    start: Option<f32>,
    end: Option<f32>,
) -> BorderRadius {
    let mut r = BorderRadius {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    };
    let rtl = use_direction().is_rtl();
    if let Some(v) = start {
        if rtl {
            (r.top_right, r.bottom_right) = (v, v);
        } else {
            (r.top_left, r.bottom_left) = (v, v);
        }
    }
    if let Some(v) = end {
        if rtl {
            (r.top_left, r.bottom_left) = (v, v);
        } else {
            (r.top_right, r.bottom_right) = (v, v);
        }
    }
    r
}

#[cfg(test)]
#[path = "border_test.rs"]
mod tests;
