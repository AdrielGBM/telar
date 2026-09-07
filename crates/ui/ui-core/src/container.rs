//! [`Container`]: the plain flex box — children, an optional tap gesture, and nothing painted.

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::{Event, PointerButton};
use reactive_core::RwSignal;
use renderer_core::Declared;
use ui_tree::{Component, EventResult, RenderNode};

use crate::child_host::{ChildSlot, DynHost};
use crate::context::{new_container, track_layout};
use crate::layout_item::{LayoutItem, TrackedChildren, register_container};
use crate::pointer::dispatch_container_event;
use crate::press::PressGesture;

/// The plain flex box: children, an optional tap gesture, and nothing painted.
pub struct Container {
    node: NodeId,
    rect: RwSignal<Rect>,
    // Empty when `dyn_host` is set: a container holding a reactive fragment routes every child through the host so they interleave in the layout node.
    children: TrackedChildren,
    dyn_host: Option<DynHost>,
    // Optional tap gesture so a plain row/col can be pressable; children still hit-test first.
    press: PressGesture,
    // What the box is, where it is more than a box. `None` reads it from what the box does.
    role: Option<renderer_core::Role>,
}

impl Container {
    pub fn new(
        layout_style: LayoutStyle,
        children: Vec<Box<dyn LayoutItem>>,
    ) -> Result<Self, LayoutError> {
        let (node, rect, children) = register_container(layout_style, children)?;
        Ok(Container {
            node,
            rect,
            children,
            dyn_host: None,
            press: PressGesture::default(),
            role: None,
        })
    }

    /// A container whose children are a mix of static widgets and reactive fragments (`ChildSlot`s). The fragments reconcile into this container's own node, so their items are real siblings of the static children and inherit this container's flex direction/gap — the transparent `for`/`if` path.
    pub fn from_slots(
        layout_style: LayoutStyle,
        slots: Vec<ChildSlot>,
    ) -> Result<Self, LayoutError> {
        let node = new_container(layout_style, &[])?;
        let rect = track_layout(node).expect("new_container always registers a signal");
        let dyn_host = DynHost::build(node, slots)?;
        Ok(Container {
            node,
            rect,
            children: Vec::new(),
            dyn_host: Some(dyn_host),
            press: PressGesture::default(),
            role: None,
        })
    }

    fn dispatch_children(&mut self, event: &Event) -> EventResult {
        match &self.dyn_host {
            Some(host) => host.dispatch(event),
            None => dispatch_container_event(&mut self.children, event),
        }
    }

    /// Says what the text below this container looks like — everything under it, not the container itself, which draws no text at all.
    ///
    /// Re-run when a signal the declaration read changes, and withdrawn when this container goes, so a subtree that is rebuilt does not inherit from the one it replaced.
    pub fn declaring(self, declared: impl Fn() -> Declared + 'static) -> Self {
        let node = self.node;
        reactive_core::effect(move || crate::inherit::declare(node, declared()));
        self
    }

    /// Keeps this container's layout style in step with the reactive state it was built from — see [`StyledContainer::styled_by`](crate::StyledContainer::styled_by), which is the same thing on a box that also paints.
    pub fn styled_by(self, style: impl Fn() -> LayoutStyle + 'static) -> Self {
        let node = self.node;
        crate::styled_container::style_follows(node, style);
        self
    }

    /// What this box *is*, beyond a box: a region of the screen, a list, a heading.
    ///
    /// Only a description — it does not make the box focusable, because a region is not something the keyboard stops at. A control says so with [`StyledContainer::control`](crate::StyledContainer::control), which declares the role *and* joins the tab order.
    ///
    /// What it buys: a screen reader that can jump between the regions of a screen, and a document backend that writes `<nav>` where a box said it was the navigation. A target that has no use for it drops it.
    pub fn role(mut self, role: renderer_core::Role) -> Self {
        self.role = Some(role);
        self
    }

    /// Make the container itself pressable. The callback fires on a tap (release, not press) inside it; a child widget that handles the press wins, and a scroll gesture started on it does not fire it.
    pub fn on_press(self, f: impl Fn() + 'static) -> Self {
        self.maybe_on_press(Some(f))
    }

    /// [`on_press`](Self::on_press) for a handler the caller may not have supplied.
    ///
    /// The emitter picks this form for any `on_press:` whose value is not a closure literal, which is how a wrapper component forwards an `Option` — and a `Container` reached that emitter with no such method, so a plain container forwarding one did not compile. `None` leaves the container untouched: a no-op handler would still report the tap `Handled`, turning a display-only row into one that swallows it.
    pub fn maybe_on_press(mut self, f: Option<impl Fn() + 'static>) -> Self {
        let Some(f) = f else { return self };
        self.press.set(f);
        self.mark_interactive();
        self
    }

    /// Registers this node in the interactive registry a click-through surface reads to carve its input region — see `StyledContainer::mark_interactive`.
    fn mark_interactive(&self) {
        crate::input_region::register_interactive(self.node, self.rect.read_only());
    }

    pub fn column(children: Vec<Box<dyn LayoutItem>>) -> Result<Self, LayoutError> {
        Self::new(LayoutStyle::new().flex_column(), children)
    }
}

impl LayoutItem for Container {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for Container {
    fn view(&self) -> RenderNode {
        // Each child is its own segment, referenced by a cheap `Rc` clone, so this `view()` neither re-runs them nor subscribes to their signals.
        let content = match &self.dyn_host {
            Some(host) => RenderNode::group(host.child_boundaries()),
            None => RenderNode::group(self.children.iter().map(|c| c.segment.boundary())),
        };
        // A transparent box still owns a layout node, and its children are laid out by it, so attaching them to its parent would put them in the wrong flow.
        if ui_tree::element_capture() {
            let semantics = renderer_core::Semantics::of(crate::element::role_of(
                self.role,
                self.press.is_set(),
            ));
            RenderNode::element(
                crate::element::with_semantics(self.node, semantics),
                [content],
            )
        } else {
            content
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // No tap handler, so this is pure child routing.
        if !self.press.is_set() {
            return self.dispatch_children(event);
        }
        let rect = self.rect.get();
        match event {
            Event::PointerMoved { .. } => {
                self.press.track_move(event);
                self.dispatch_children(event)
            }
            Event::PointerPressed {
                button: PointerButton::Primary,
                ..
            } => {
                if self.dispatch_children(event) == EventResult::Handled {
                    self.press.cancel();
                    return EventResult::Handled;
                }
                self.press.arm(event, rect)
            }
            Event::PointerReleased {
                button: PointerButton::Primary,
                ..
            } => {
                if self.dispatch_children(event) == EventResult::Handled {
                    self.press.cancel();
                    return EventResult::Handled;
                }
                self.press.release(event, rect)
            }
            // Neither delivers the release a tap needs, and a press left armed would pair with the next release and fire a click nobody made.
            Event::CursorLeft | Event::FocusChanged { is_focused: false } => {
                self.press.cancel();
                self.dispatch_children(event)
            }
            _ => self.dispatch_children(event),
        }
    }

    fn debug_name(&self) -> &'static str {
        "Container"
    }
}

impl Drop for Container {
    fn drop(&mut self) {
        crate::input_region::unregister_interactive(self.node);
    }
}

#[cfg(test)]
#[path = "container_test.rs"]
mod tests;
