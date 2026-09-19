//! First-last-invert-play: the node is displaced by the exact distance layout moved it, then that translation animates back to zero, so nothing relays out and clip rects stay put while it plays; position is measured against the layout parent so nested transitions don't compound.

use std::cell::Cell;
use std::rc::Rc;

use geometry_core::Point;
use layout_core::NodeId;
use motion_core::{Animated, Curve, Lerp};
use platform_core::Event;
use reactive_core::{Effect, effect, on_cleanup};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{remove_node, track_layout};
use crate::input_region::{InputHandle, Placement};
use crate::layout_item::{Child, LayoutItem, make_child};

pub(crate) struct Flip {
    offset: Animated<Point>,
    out_of_flow: Rc<Cell<bool>>,
    _watch: Effect,
    _input: InputHandle,
}

impl Flip {
    /// Watches `node` from here on. Belongs to the active owner, like the animation it drives.
    pub(crate) fn new(node: NodeId, curve: Curve) -> Self {
        let offset = Animated::new(Point::zero(), curve);
        let out_of_flow = Rc::new(Cell::new(false));
        let held_out = Rc::clone(&out_of_flow);
        let last = Cell::new(None);
        let _watch = effect(move || {
            if !offset.is_alive() {
                return;
            }
            // Out of flow its rect is frozen where it was drawn and it hangs from nothing, so the baseline is kept against the parent it left: that parent moving is what moves the baseline, and rejoining it slides the node from there.
            let out = held_out.get();
            let left = last.get().and_then(|(parent, _)| parent).filter(|_| out);
            let now = position_in_parent(node, left);
            let before = last.replace(now);
            if out {
                return;
            }
            if let (Some((parent, at)), Some((was_under, was_at))) = (now, before)
                && parent == was_under
            {
                offset.displace(was_at.sub(&at));
            }
        });
        // A node out of its parent's flow is not among the parent's layout children, so freeing the parent's subtree would miss it.
        let orphan = Rc::clone(&out_of_flow);
        on_cleanup(move || {
            if orphan.replace(false) {
                remove_node(node);
            }
        });
        let mut input = InputHandle::new();
        input.place(
            node,
            Placement::Offset(Rc::new(move || {
                let at = offset_now(offset);
                (at.x, at.y)
            })),
        );
        Self {
            offset,
            out_of_flow,
            _watch,
            _input: input,
        }
    }

    /// The translation to draw with, subscribing the caller to it.
    pub(crate) fn offset(&self) -> Point {
        if !self.offset.is_alive() {
            return Point::zero();
        }
        self.offset.get()
    }

    pub(crate) fn offset_now(&self) -> Point {
        offset_now(self.offset)
    }

    pub(crate) fn set_out_of_flow(&self, out: bool) {
        self.out_of_flow.set(out);
    }
}

fn offset_now(offset: Animated<Point>) -> Point {
    if !offset.is_alive() {
        return Point::zero();
    }
    offset.read().peek()
}

/// Where `node` sits inside its layout parent, and which parent that is — `left`, the one it was taken out of, while it hangs from none. `None` until it has been laid out with an area, since a node with none is drawn nowhere and has nowhere to slide from.
fn position_in_parent(node: NodeId, left: Option<NodeId>) -> Option<(Option<NodeId>, Point)> {
    let rect = track_layout(node)?.try_get()?;
    if rect.width <= 0.0 && rect.height <= 0.0 {
        return None;
    }
    let parent = layout_reactive::parent(node).or(left);
    let origin = parent
        .and_then(track_layout)
        .and_then(|rect| rect.try_get())
        .map_or(Point::zero(), |rect| Point::new(rect.x, rect.y));
    Some((parent, Point::new(rect.x - origin.x, rect.y - origin.y)))
}

/// `matrix`, then a translation by `offset`.
pub(crate) fn translated(matrix: [f32; 6], offset: Point) -> [f32; 6] {
    if offset.x == 0.0 && offset.y == 0.0 {
        return matrix;
    }
    let [a, b, c, d, e, f] = matrix;
    [a, b, c, d, e + offset.x, f + offset.y]
}

/// Transform-only: laid out once at its new place and drawn gliding there under `curve`, so a size change still lands at once and a time scale of zero (reduced motion) skips the glide. Built by [`animate_layout`].
pub struct LayoutTransition {
    child: Child,
}

/// Wraps `item` so that it slides to wherever the layout moves it, under `curve`. A spring keeps its momentum when a second move interrupts the first; a tween takes the share of its duration the remaining distance calls for.
pub fn animate_layout(
    item: impl LayoutItem + 'static,
    curve: impl Into<Curve>,
) -> LayoutTransition {
    let mut child = make_child(Box::new(item));
    child.animate_layout(curve.into());
    LayoutTransition { child }
}

impl LayoutItem for LayoutTransition {
    fn layout_node(&self) -> NodeId {
        self.child.node()
    }

    fn occludes(&self) -> bool {
        self.child
            .item
            .try_borrow()
            .is_ok_and(|item| item.occludes())
    }
}

impl Component for LayoutTransition {
    fn view(&self) -> RenderNode {
        self.child.boundary()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.child.deliver(event)
    }

    fn debug_name(&self) -> &'static str {
        "LayoutTransition"
    }
}

#[cfg(test)]
#[path = "layout_transition_test.rs"]
mod tests;
