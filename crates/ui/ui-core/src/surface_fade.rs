//! [`SurfaceFade`]: content that fades in with a [`SurfaceTransition`], and back out when it leaves.

use geometry_core::Transform;
use layout_core::NodeId;
use platform_core::Event;
use ui_tree::{Component, EventResult, RenderNode};

use crate::layout_item::LayoutItem;
use crate::surface::{EnterMotion, SurfaceTransition, apply_enter, enter_transform};

/// Draws `content` at the opacity its transition has reached: how a whole surface fades in as it opens, and out again when its owner calls [`SurfaceTransition::leave`].
///
/// Adds nothing to the layout: the content's own node is the one laid out, so wrapping a window's content in it changes how it is drawn and nothing else.
pub struct SurfaceFade {
    content: Box<dyn LayoutItem>,
    transition: SurfaceTransition,
}

impl SurfaceFade {
    /// `content` driven by `transition`: [`SurfaceTransition::enter`] for one that only arrives, or a clone of one the caller keeps to send the surface back out.
    pub fn new(content: Box<dyn LayoutItem>, transition: SurfaceTransition) -> Self {
        Self {
            content,
            transition,
        }
    }
}

impl LayoutItem for SurfaceFade {
    fn layout_node(&self) -> NodeId {
        self.content.layout_node()
    }
}

impl Component for SurfaceFade {
    fn view(&self) -> RenderNode {
        let (_, opacity) = enter_transform(EnterMotion::Fade, self.transition.get());
        apply_enter(self.content.view(), Transform::IDENTITY, opacity)
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.content.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        "SurfaceFade"
    }
}

#[cfg(test)]
#[path = "surface_fade.test.rs"]
mod tests;
