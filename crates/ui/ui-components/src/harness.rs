//! The pointer-event and layout scaffolding every widget's test module was writing for itself.
//!
//! Ten copies of `press`, nine of `release` and seven of `lay_out` had gone byte-identical — `checkbox`, `radio` and `toggle` shared 48 consecutive lines. Two things stayed parameters rather than being folded in, because folding them would have changed what the tests measure: the root's **direction** (`tabs` lays its pills out in a row, everyone else in a column, and a single `flex_column` helper would silently move the rect it asserts on) and the root's **size**.

use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{Event, PointerButton, PointerSource};
use ui_core::{
    ComponentList, EventResult, NodeId, compute_layout, dispatch_overlays, new_container,
};

pub(crate) fn press(x: f64, y: f64) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

pub(crate) fn release(x: f64, y: f64) -> Event {
    Event::PointerReleased {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

pub(crate) fn moved(x: f64, y: f64) -> Event {
    Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    }
}

/// Dispatches like the runner does: overlays first, and only what they ignore reaches the tree. A widget with an open panel is not reachable any other way — the panel is in the overlay layer, not under the root.
pub(crate) fn route(tree: &mut ComponentList, event: &Event) {
    if dispatch_overlays(event) == EventResult::Ignored {
        tree.on_event(event);
    }
}

/// Lays `node` out as the only child of a `w`x`h` root and hands back its absolute rect.
pub(crate) fn lay_out(node: NodeId, w: f32, h: f32) -> Rect {
    lay_out_in(
        LayoutStyle::new().flex_column().width(w).height(h),
        node,
        w,
        h,
    )
}

/// [`lay_out`] with a row root, for a widget whose children are laid out along the main axis.
pub(crate) fn lay_out_row(node: NodeId, w: f32, h: f32) -> Rect {
    lay_out_in(LayoutStyle::new().flex_row().width(w).height(h), node, w, h)
}

fn lay_out_in(root_style: LayoutStyle, node: NodeId, w: f32, h: f32) -> Rect {
    let rect = ui_core::track_layout(node).unwrap();
    let root = new_container(root_style, &[node]).unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(w),
        AvailableSpace::Definite(h),
    )
    .unwrap();
    rect.get()
}

pub(crate) fn centre(rect: Rect) -> (f64, f64) {
    (
        (rect.x + rect.width / 2.0) as f64,
        (rect.y + rect.height / 2.0) as f64,
    )
}
