//! [`FixedLayer`]: boxes that stand against the surface over the page, out of its flow and its scroll, and take the pointer only where they are.

use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use renderer_core::Semantics;
use ui_tree::{
    Component, EventResult, OverlaySink, RenderNode, register_overlay, unregister_overlay,
};

use crate::context::{new_leaf, remove_node};
use crate::input_region::{
    InputHandle, pointable, receives_input, tracks_hidden, visible_rect, withhold,
};
use crate::layout_item::{LayoutItem, TrackedChildren, register_container};
use crate::pointer::dispatch_container_event;

/// A non-modal layer fixed to the surface, drawn over the primary scroll: a bar that stays at the top of the window while the page scrolls under it.
///
/// Its content is a layout root of its own, laid out at the surface's size and origin from the first frame (see [`layout_reactive::lay_out_against_surface`]), so `align`/`justify`/`pad` and the children's own sizes place its boxes against the surface. The page never scrolls it, its sticky boxes never cover it, and it takes no room where it is declared.
///
/// It is not a modal, which is what sets it apart from an [`Overlay`](crate::Overlay):
///
/// - **Pointer.** It is hit-tested before the page, but only over its children's boxes. A press anywhere else, the empty part of the surface it spans included, reaches the page under it.
/// - **Keyboard.** Its focusables keep their place in the Tab order, where the layer was declared, and focus walks in and out of it freely.
/// - **Stacking.** It is drawn over every box of the page, sticky ones included, and under every overlay, whatever order the two were declared in. Layers among themselves stack in the order they were declared.
/// - **Arrival.** A box of it that stands against the top or the bottom edge of the surface, and not both, is a bar over that edge, and a [`ScrollPage`](crate::ScrollPage) that declares no arrival margin of its own stops what it brings into view short of it.
///
/// Like an overlay, its content starts the cascade over from the surface rather than from the boxes around where it is declared, so a `theme:` or a text style meant for it goes on the layer or inside it.
pub struct FixedLayer {
    /// What its parent holds where it was declared: a box out of the flow and of no size, which takes no room and adds no gap.
    place: NodeId,
    content: NodeId,
    children: TrackedChildren,
    overlay_id: u64,
    _input: InputHandle,
    _covers: crate::scroll_viewports::LayerRegistration,
}

impl FixedLayer {
    pub fn new(
        layout_style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let (content, _rect, children) = register_container(layout_style, children)?;
        layout_reactive::lay_out_against_surface(content);
        let (place, _rect) = new_leaf(LayoutStyle::new().absolute().width(0.0).height(0.0))?;
        let sink: Rc<dyn OverlaySink> = Rc::new(LayerSink {
            children: RefCell::new(children.clone()),
            content,
        });
        let overlay_id = register_overlay(sink);
        let mut input = InputHandle::new();
        // Linked as hoisted: whether it takes input is asked of where it was declared, but its geometry is the surface's own.
        input.link(content, place, true);
        let covers = crate::scroll_viewports::register_layer(
            children.iter().map(|child| child.node()).collect(),
        );
        Ok(Self {
            place,
            content,
            children,
            overlay_id,
            _input: input,
            _covers: covers,
        })
    }

    /// The root its boxes hang from, laid out against the surface.
    pub fn content_node(&self) -> NodeId {
        self.content
    }
}

/// The layer's hook into priority pointer routing, sharing the widget's child handles.
struct LayerSink {
    children: RefCell<TrackedChildren>,
    content: NodeId,
}

impl OverlaySink for LayerSink {
    fn content_rect(&self) -> Rect {
        self.children
            .borrow()
            .iter()
            .filter_map(|child| visible_rect(child.node()))
            .reduce(Rect::union)
            .unwrap_or_default()
    }

    fn dispatch(&self, event: &Event) -> EventResult {
        dispatch_container_event(&mut self.children.borrow_mut(), event)
    }

    fn hits(&self, x: f32, y: f32) -> bool {
        receives_input(self.content)
            && self
                .children
                .borrow()
                .iter()
                .any(|child| receives_input(child.node()) && pointable(child.node(), x, y))
    }

    fn fixed(&self) -> bool {
        true
    }
}

impl LayoutItem for FixedLayer {
    fn layout_node(&self) -> NodeId {
        self.place
    }

    /// Reached through the overlay registry ahead of the tree walk; its place in the tree covers nothing.
    fn occludes(&self) -> bool {
        false
    }
}

impl Component for FixedLayer {
    fn view(&self) -> RenderNode {
        let place = crate::element::for_target(self.place, renderer_core::Role::Group, || {
            crate::element::with_semantics(self.place, Semantics::group())
        });
        // Its content is a root of its own, so a box hiding the place it was declared in does not hide it on its own.
        if tracks_hidden(self.place) {
            return RenderNode::element(place, []);
        }
        let content = crate::element::for_target(self.content, renderer_core::Role::Group, || {
            crate::element::fixed_in_place_of(self.content, self.place)
        });
        let boundaries = self.children.iter().map(|child| child.segment.boundary());
        RenderNode::group([
            RenderNode::element(place, []),
            RenderNode::fixed([RenderNode::element(content, boundaries)]),
        ])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // Positioned pointer events arrive through the overlay registry, in surface coordinates; the tree walk would hand them over a second time, in the page's.
        if matches!(
            event,
            Event::PointerPressed { .. }
                | Event::PointerMoved { .. }
                | Event::PointerReleased { .. }
        ) {
            return EventResult::Ignored;
        }
        if !receives_input(self.content) {
            return withhold(event, |event| {
                dispatch_container_event(&mut self.children, event)
            });
        }
        dispatch_container_event(&mut self.children, event)
    }

    fn debug_name(&self) -> &'static str {
        "FixedLayer"
    }
}

impl Drop for FixedLayer {
    fn drop(&mut self) {
        unregister_overlay(self.overlay_id);
        // A root of its own, so nothing above it would free it.
        remove_node(self.content);
    }
}

#[cfg(test)]
#[path = "fixed_layer_test.rs"]
mod tests;
