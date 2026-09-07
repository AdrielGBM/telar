//! [`Lazy`]: a subtree held back until its condition is first true, then kept.

use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::{Effect, RwSignal, effect, signal};
use ui_tree::{Component, EventResult, RenderNode};

use crate::context::{mark_dirty, new_container, set_children, set_display, track_layout};
use crate::layout_item::{LayoutItem, TrackedChildren, make_child};
use crate::pointer::dispatch_container_event;

/// The deferred subtree, taken out of the cell and run the first time the block is shown.
type LazyBuild = Box<dyn FnOnce() -> Result<Vec<Box<dyn LayoutItem>>, LayoutError>>;

struct LazyState {
    node: NodeId,
    children: TrackedChildren,
    build: Option<LazyBuild>,
}

/// A subtree that is not built until the first time it would be shown — `lazy when:$cond { … }` in `.rsx`.
///
/// This is the general form of what a [`NavHost`](../../navigate_core/struct.NavHost.html) does per route: pay for a screen when the user first reaches it, not at startup. Use it for anything expensive behind a condition the user may never satisfy — a settings panel, an inspector, a tab body, a chart that only some accounts see.
///
/// It is deliberately *not* what a reactive `if $cond` does. That builds its branch whenever the condition turns true and disposes it when it turns false, so a repeatedly toggled panel is rebuilt every time and loses whatever state it held. This builds **once**, on the first `true`, and from then on only shows or hides the same subtree — so scroll position, form entry and in-flight work survive being closed and reopened. The cost is symmetric: a subtree shown once is held until the whole block is dropped.
pub struct Lazy {
    node: NodeId,
    rect: RwSignal<Rect>,
    state: Rc<RefCell<LazyState>>,
    visible: Rc<dyn Fn() -> bool>,
    /// Bumped when the subtree is finally built, so `view()` (which reads it) re-emits with real children.
    version: RwSignal<u64>,
    _effect: Effect,
}

impl Lazy {
    /// `visible` is the reactive condition; `build` constructs the children the first time it holds, against the live layout tree from inside the tracking effect — the same way a reactive list builds its items.
    pub fn new(
        container_style: LayoutStyle,
        visible: impl Fn() -> bool + 'static,
        build: impl FnOnce() -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> + 'static,
    ) -> Result<Self, LayoutError> {
        let node = new_container(container_style, &[])?;
        let rect = track_layout(node).expect("lazy container is registered");
        let state = Rc::new(RefCell::new(LazyState {
            node,
            children: Vec::new(),
            build: Some(Box::new(build)),
        }));
        let version = signal(0u64);
        let visible: Rc<dyn Fn() -> bool> = Rc::new(visible);

        let eff_state = Rc::clone(&state);
        let eff_version = version;
        let eff_visible = Rc::clone(&visible);
        // Runs once now, which is what makes an initially-false block cost nothing, and again on every change.
        let _effect = effect(move || {
            let show = eff_visible();
            if show && realize(&eff_state) {
                eff_version.update(|v| *v = v.wrapping_add(1));
            }
            set_display(node, show);
            mark_dirty(node).ok();
        });

        Ok(Self {
            node,
            rect,
            state,
            visible,
            version,
            _effect,
        })
    }

    /// Whether the subtree has been built yet — false until the condition first holds.
    pub fn is_built(&self) -> bool {
        self.state.borrow().build.is_none()
    }
}

/// Builds the deferred children if this is the first showing, reporting whether it did any work. Taking the builder out of the cell is what makes it once-only: every later showing finds `None` and just toggles display.
fn realize(state: &Rc<RefCell<LazyState>>) -> bool {
    let Some(build) = state.borrow_mut().build.take() else {
        return false;
    };
    // Built outside the state borrow: constructing widgets reads and writes signals whose effects can reach back into this cell.
    let Ok(items) = build() else {
        return false;
    };
    let children: TrackedChildren = items.into_iter().map(make_child).collect();
    let nodes: Vec<NodeId> = children.iter().map(|c| c.node()).collect();

    let mut st = state.borrow_mut();
    st.children = children;
    let container = st.node;
    drop(st);
    let _ = set_children(container, &nodes);
    true
}

impl LayoutItem for Lazy {
    fn layout_node(&self) -> NodeId {
        self.node
    }
}

impl Component for Lazy {
    fn view(&self) -> RenderNode {
        // Both: the condition, so hiding re-renders without children, and the build, so the first showing re-emits.
        let show = (self.visible)();
        self.version.get();
        let _ = self.rect.get();
        if !show {
            return RenderNode::Empty;
        }
        let st = self.state.borrow();
        let content = RenderNode::group(st.children.iter().map(|c| c.segment.boundary()));
        crate::element::wrap(self.node, content)
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // A hidden block takes no space, so it must not answer for the content shown over it.
        if !(self.visible)() {
            return EventResult::Ignored;
        }
        let mut st = self.state.borrow_mut();
        dispatch_container_event(&mut st.children, event)
    }

    fn debug_name(&self) -> &'static str {
        "Lazy"
    }
}

#[cfg(test)]
#[path = "lazy_test.rs"]
mod tests;
