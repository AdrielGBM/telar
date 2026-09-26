//! Sticky positioning: where a box that sticks is drawn, given the window it is seen through.
//!
//! Layout never moves a sticky box — it takes its place in the flow like any other — so this is a pass run after it, against the *sticky view*: the part of the laid-out tree a scroll viewport is showing, in the tree's own coordinates. A box sticks to the edges its insets name and never leaves its containing block, as CSS `position: sticky` does.

use geometry_core::Rect;
use taffy::{CompactLength, LengthPercentageAuto};

use crate::style::SizeDimension;

/// The physical edges a sticky box keeps its distance from, each `None` when that edge does not stick. A percentage is a fraction of the view along the same axis, as CSS resolves it against the scrollport.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct StickyInsets {
    pub(crate) top: Option<SizeDimension>,
    pub(crate) right: Option<SizeDimension>,
    pub(crate) bottom: Option<SizeDimension>,
    pub(crate) left: Option<SizeDimension>,
}

impl StickyInsets {
    /// The insets a resolved style carries: already physical, already in pixels where they were a fraction of the surface.
    pub(crate) fn of(inset: taffy::Rect<LengthPercentageAuto>) -> Self {
        Self {
            top: edge(inset.top),
            right: edge(inset.right),
            bottom: edge(inset.bottom),
            left: edge(inset.left),
        }
    }
}

fn edge(value: LengthPercentageAuto) -> Option<SizeDimension> {
    let raw = value.into_raw();
    match raw.tag() {
        CompactLength::LENGTH_TAG => Some(SizeDimension::Px(raw.value())),
        CompactLength::PERCENT_TAG => Some(SizeDimension::Percent(raw.value())),
        _ => None,
    }
}

fn pixels(inset: Option<SizeDimension>, basis: f32) -> Option<f32> {
    match inset? {
        SizeDimension::Px(px) => Some(px),
        SizeDimension::Percent(fraction) => Some(fraction * basis),
        _ => None,
    }
}

/// A box's margins, which keep it that far inside its containing block while it sticks.
pub(crate) type Margins = taffy::Rect<f32>;

/// How far a sticky box is displaced from where layout put it, as `(dx, dy)`.
///
/// `laid_out` is the box's border box, `container` its containing block's content box and `view` the sticky view, all in the same coordinates. The box moves as far as it has to for its border box to stay the named distance inside the view, and no further than keeps its margin box inside the container. When both edges of an axis stick and the box cannot honour both, the top and the left win.
pub(crate) fn displacement(
    insets: &StickyInsets,
    laid_out: Rect,
    margins: Margins,
    container: Rect,
    view: Rect,
) -> (f32, f32) {
    let dx = along(
        Axis {
            start: laid_out.x,
            size: laid_out.width,
            margin_start: margins.left,
            margin_end: margins.right,
            container_start: container.x,
            container_size: container.width,
            view_start: view.x,
            view_size: view.width,
        },
        pixels(insets.left, view.width),
        pixels(insets.right, view.width),
    );
    let dy = along(
        Axis {
            start: laid_out.y,
            size: laid_out.height,
            margin_start: margins.top,
            margin_end: margins.bottom,
            container_start: container.y,
            container_size: container.height,
            view_start: view.y,
            view_size: view.height,
        },
        pixels(insets.top, view.height),
        pixels(insets.bottom, view.height),
    );
    (dx, dy)
}

/// One axis of the problem, so the vertical and horizontal cases are one rule.
struct Axis {
    start: f32,
    size: f32,
    margin_start: f32,
    margin_end: f32,
    container_start: f32,
    container_size: f32,
    view_start: f32,
    view_size: f32,
}

fn along(axis: Axis, start_inset: Option<f32>, end_inset: Option<f32>) -> f32 {
    let lowest = axis.container_start + axis.margin_start;
    let highest = axis.container_start + axis.container_size - axis.margin_end - axis.size;
    let mut at = axis.start;
    // The end edge first, so the start edge has the last word when the two disagree. Each clamp only ever moves the box towards the edge it sticks to: a box already outside its container is left there rather than pulled back in.
    if let Some(inset) = end_inset {
        let stuck = axis.view_start + axis.view_size - inset - axis.size;
        at = at.min(stuck.max(lowest));
    }
    if let Some(inset) = start_inset {
        let stuck = axis.view_start + inset;
        at = at.max(stuck.min(highest));
    }
    at - axis.start
}

#[cfg(test)]
#[path = "sticky_test.rs"]
mod tests;
