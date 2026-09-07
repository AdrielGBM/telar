//! The root a windowed application mounts.

use layout_core::{AvailableSpace, LayoutError, LayoutStyle, NodeId, SizeDimension};
use platform_core::Event;
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{compute_layout, mark_dirty, new_container};
use crate::layout_item::LayoutItem;
use crate::surface::{EnterMotion, IDENTITY, SurfaceTransition, apply_enter, enter_transform};

/// Lays its content out against the window, because nothing above it will.
///
/// A percent-sized tree resolves to nothing until something hands it a definite space, and Telar hands the tree a window rather than laying it out. Without this root the content's rects stay zero and the window is black forever, which looks exactly like a renderer that never drew.
///
/// [`ScrollPage`](crate::ScrollPage) is the same shape for a window that is one scrolling column.
pub struct WindowRoot {
    root: NodeId,
    content: Box<dyn LayoutItem>,
    transition: Option<SurfaceTransition>,
}

impl WindowRoot {
    /// Lays `content`'s own node out against the window, adding nothing to the tree.
    ///
    /// `content` must size itself to fill the window — a percent-sized box is the usual answer. For content that sizes itself to its children instead, use [`WindowRoot::wrapping`].
    pub fn new(content: Box<dyn LayoutItem>) -> Self {
        Self {
            root: content.layout_node(),
            content,
            transition: None,
        }
    }

    /// Wraps `content` in a window-filling box and lays *that* out, for content that does not fill the window on its own or that has to stretch inside a parent of a fixed size.
    pub fn wrapping(content: Box<dyn LayoutItem>) -> Result<Self, LayoutError> {
        let root = new_container(
            LayoutStyle::new()
                .flex_row()
                .width(SizeDimension::Percent(1.0))
                .height(SizeDimension::Percent(1.0)),
            &[content.layout_node()],
        )?;
        Ok(Self {
            root,
            content,
            transition: None,
        })
    }

    pub fn animate_in(self) -> Self {
        self.animate(SurfaceTransition::enter())
    }

    /// Drives the root from a transition the *caller* owns, so it can also send the surface back out — see [`SurfaceTransition::leave`].
    pub fn animate(mut self, transition: SurfaceTransition) -> Self {
        self.transition = Some(transition);
        self
    }
}

impl LayoutItem for WindowRoot {
    fn layout_node(&self) -> NodeId {
        self.root
    }
}

impl Component for WindowRoot {
    fn view(&self) -> RenderNode {
        let content = self.content.view();
        match &self.transition {
            Some(transition) => {
                let (_, opacity) = enter_transform(EnterMotion::Fade, transition.get());
                apply_enter(content, IDENTITY, opacity)
            }
            None => content,
        }
    }

    /// Lays out first, then passes the resize on, so anything that has to run once the tree has real rects — a scroll viewport that is its own layout root, a first-layout autofocus — sees it in that order.
    ///
    /// `Handled` regardless of what the content answered: the runner requests a redraw only for a handled event (`runner::handler`), so reporting the content's `Ignored` would relayout and never repaint.
    fn on_event(&mut self, event: &Event) -> EventResult {
        // Before the press goes down the tree, so whatever is pressed takes focus back on the way through and only a press with nothing focusable under it leaves the keyboard unheld.
        if let Event::PointerPressed {
            x,
            y,
            button: platform_core::PointerButton::Primary,
            ..
        } = event
        {
            crate::focus::blur_from_pointer(*x as f32, *y as f32);
        }
        // Tab is answered by the box that holds focus, which leaves the first one: with nothing focused the key reaches nobody. Asked last, so a control that wants Tab for itself has already taken it.
        if let Event::KeyPressed { key, modifiers } = event
            && matches!(key, platform_core::Key::Named(platform_core::NamedKey::Tab))
            && crate::focus::current().is_none()
        {
            if self.content.on_event(event) == EventResult::Handled {
                return EventResult::Handled;
            }
            if modifiers.is_shift {
                crate::focus::focus_prev();
            } else {
                crate::focus::focus_next();
            }
            return EventResult::Handled;
        }
        if let Event::WindowResized { width, height } = event {
            mark_dirty(self.root).ok();
            compute_layout(
                self.root,
                AvailableSpace::Definite(*width as f32),
                AvailableSpace::Definite(*height as f32),
            )
            .ok();
            self.content.on_event(event);
            return EventResult::Handled;
        }
        self.content.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        "WindowRoot"
    }
}

#[cfg(test)]
#[path = "window_root_test.rs"]
mod tests;
