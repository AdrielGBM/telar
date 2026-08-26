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
/// This is the general form of what a [`NavHost`](../../navigate_core/struct.NavHost.html) does per route:
/// pay for a screen when the user first reaches it, not at startup. Use it for anything expensive behind a
/// condition the user may never satisfy — a settings panel, an inspector, a tab body, a chart that only some
/// accounts see.
///
/// It is deliberately *not* what a reactive `if $cond` does. That builds its branch whenever the condition
/// turns true and disposes it when it turns false, so a repeatedly toggled panel is rebuilt every time and
/// loses whatever state it held. This builds **once**, on the first `true`, and from then on only shows or
/// hides the same subtree — so scroll position, form entry and in-flight work survive being closed and
/// reopened. The cost is symmetric: a subtree shown once is held until the whole block is dropped.
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
    /// `visible` is the reactive condition; `build` constructs the children the first time it holds, against
    /// the live layout tree from inside the tracking effect — the same way a reactive list builds its items.
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
        // Runs once now — which is what makes an initially-false block cost nothing — and again on every change to a signal the condition reads.
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

/// Builds the deferred children if this is the first showing, reporting whether it did any work. Taking the
/// builder out of the cell is what makes it once-only: every later showing finds `None` and just toggles
/// display.
fn realize(state: &Rc<RefCell<LazyState>>) -> bool {
    let Some(build) = state.borrow_mut().build.take() else {
        return false;
    };
    // Built outside the state borrow: constructing widgets reads and writes signals, whose effects can reach back into this same cell.
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
        // Subscribe to both: the condition (so hiding re-renders without children) and the build (so the first showing re-emits with them).
        let show = (self.visible)();
        self.version.get();
        let _ = self.rect.get();
        if !show {
            return RenderNode::Empty;
        }
        let st = self.state.borrow();
        RenderNode::group(st.children.iter().map(|c| c.segment.boundary()))
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        // A hidden block is inert: it takes no space, so it must not answer for the content shown over it.
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
mod tests {
    use std::cell::Cell;

    use layout_core::AvailableSpace;
    use reactive_core::signal;

    use super::*;
    use crate::container::Container;
    use crate::context::{compute_layout, reset_layout_runtime};

    fn leaf() -> Result<Box<dyn LayoutItem>, LayoutError> {
        Ok(Box::new(Container::new(
            LayoutStyle::new().width(10.0).height(10.0),
            vec![],
        )?))
    }

    #[test]
    fn defers_construction_until_the_condition_first_holds() {
        reset_layout_runtime();
        let show = signal(false);
        let builds = Rc::new(Cell::new(0));
        let lazy = {
            let (cond, builds) = (show, builds.clone());
            Lazy::new(
                LayoutStyle::new().flex_column(),
                move || cond.get(),
                move || {
                    builds.set(builds.get() + 1);
                    Ok(vec![leaf()?])
                },
            )
            .unwrap()
        };
        assert_eq!(builds.get(), 0, "a block never shown costs nothing");
        assert!(!lazy.is_built());
        assert!(matches!(lazy.view(), RenderNode::Empty));

        show.set(true);
        assert_eq!(builds.get(), 1, "the first showing builds the subtree");
        assert!(lazy.is_built());
    }

    /// The difference from a reactive `if`: toggling off and on again shows the *same* subtree rather than
    /// disposing and rebuilding it, so anything it held is still there.
    #[test]
    fn builds_once_and_only_toggles_afterwards() {
        reset_layout_runtime();
        let show = signal(true);
        let builds = Rc::new(Cell::new(0));
        let lazy = {
            let (cond, builds) = (show, builds.clone());
            Lazy::new(
                LayoutStyle::new().flex_column(),
                move || cond.get(),
                move || {
                    builds.set(builds.get() + 1);
                    Ok(vec![leaf()?])
                },
            )
            .unwrap()
        };
        assert_eq!(builds.get(), 1, "an initially-true block builds at once");
        let node = lazy.state.borrow().children[0].node();

        show.set(false);
        show.set(true);
        show.set(false);
        show.set(true);
        assert_eq!(builds.get(), 1, "reopening never rebuilds");
        assert_eq!(
            lazy.state.borrow().children[0].node(),
            node,
            "it is the same subtree, not a fresh one"
        );
    }

    #[test]
    fn a_hidden_block_takes_no_space() {
        reset_layout_runtime();
        let show = signal(true);
        let lazy = {
            let cond = show;
            Lazy::new(
                LayoutStyle::new().flex_column(),
                move || cond.get(),
                || Ok(vec![leaf()?]),
            )
            .unwrap()
        };
        let node = lazy.layout_node();
        compute_layout(
            node,
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        assert!(track_layout(node).unwrap().get().height > 0.0);

        show.set(false);
        compute_layout(
            node,
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();
        assert_eq!(track_layout(node).unwrap().get().height, 0.0);
    }

    #[test]
    fn a_hidden_block_ignores_events() {
        reset_layout_runtime();
        let show = signal(false);
        let mut lazy = {
            let cond = show;
            Lazy::new(
                LayoutStyle::new().flex_column(),
                move || cond.get(),
                || Ok(vec![leaf()?]),
            )
            .unwrap()
        };
        assert_eq!(
            lazy.on_event(&Event::CursorEntered),
            EventResult::Ignored,
            "an unbuilt block answers for nothing"
        );
    }
}
