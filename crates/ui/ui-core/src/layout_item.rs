use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::RwSignal;
use ui_tree::{Component, EventResult, RenderNode, Segment};

use crate::context::{WidgetCtx, new_container, track_layout};
use crate::layout_leaf::LayoutLeaf;

/// A container child. The boxed widget is shared (`Rc<RefCell<…>>`) between event dispatch (which
/// borrows it mutably) and its render `segment` (which borrows it immutably to flatten its `view()`)
/// — they never overlap because dispatch is batched. `rect` is the child's layout signal for hit-testing.
/// `Clone` is a cheap handle copy (all fields are `Rc`/signal): a reactive list clones a `Child` to move
/// a reused item to its new position without rebuilding it.
#[derive(Clone)]
pub(crate) struct Child {
    pub(crate) item: Rc<RefCell<Box<dyn LayoutItem>>>,
    pub(crate) rect: Option<RwSignal<Rect>>,
    pub(crate) segment: Rc<Segment>,
}

impl Child {
    /// The child's layout node, read through the shared widget.
    pub(crate) fn node(&self) -> layout_core::NodeId {
        self.item.borrow().layout_node()
    }
}

/// Registers an already-built widget as a container child: tracks its layout rect and mounts its render
/// segment. Used by reactive lists to fold a freshly-built item into the child set (the per-item half of
/// [`register_container`]).
pub(crate) fn make_child(widget: Box<dyn LayoutItem>) -> Child {
    let rect = track_layout(&WidgetCtx::handle(), widget.layout_node());
    let item = Rc::new(RefCell::new(widget));
    let segment = mount_item_segment(Rc::clone(&item));
    Child {
        item,
        rect,
        segment,
    }
}

pub(crate) type TrackedChildren = Vec<Child>;

/// Mounts a reactive segment that renders a shared boxed item via its `view()`. Uses `try_borrow`
/// so a re-entrant render while the item is mid event-dispatch (mutably borrowed) keeps the previous
/// frame instead of panicking; a later flush re-runs it.
pub(crate) fn mount_item_segment(item: Rc<RefCell<Box<dyn LayoutItem>>>) -> Rc<Segment> {
    let name = item
        .try_borrow()
        .map(|i| i.debug_name())
        .unwrap_or("Component");
    Segment::mount_fn_named(name, move || item.try_borrow().ok().map(|i| i.view()))
}

pub(crate) trait LeafWidget {
    fn layout_leaf(&self) -> &LayoutLeaf;
}

pub trait LayoutItem: Component {
    fn layout_node(&self) -> NodeId;
}

/// Wraps a child so its rendered output is clipped to the child's own layout rect. When the child
/// collapses to a zero rect (e.g. a section hidden via `display:none`), the clip is empty, so nothing
/// inside draws — even a widget left with a stale rect or one that paints at fixed coordinates. Layout
/// is unchanged: `layout_node` passes through to the wrapped child.
pub struct ClippedItem {
    inner: Box<dyn LayoutItem>,
    rect: RwSignal<Rect>,
}

impl ClippedItem {
    pub fn new(ctx: &WidgetCtx, inner: Box<dyn LayoutItem>) -> Self {
        let rect =
            track_layout(ctx, inner.layout_node()).expect("clipped item's node not registered");
        Self { inner, rect }
    }
}

impl LayoutItem for ClippedItem {
    fn layout_node(&self) -> NodeId {
        self.inner.layout_node()
    }
}

impl Component for ClippedItem {
    fn view(&self) -> RenderNode {
        RenderNode::Clip {
            rect: self.rect.get(),
            radius: renderer_core::BorderRadius::zero(),
            children: ui_tree::NodeVec::collect([self.inner.view()]),
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.inner.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        "Clipped"
    }
}

impl<T: LeafWidget + Component> LayoutItem for T {
    fn layout_node(&self) -> NodeId {
        self.layout_leaf().node
    }
}

// Lets an already-boxed child (e.g. the `Box<dyn LayoutItem>` returned by a transpiled `.rsx` component) pass back through `box_item`/`children!` without a second manual wrap, so components compose as `[view]` children.
impl Component for Box<dyn LayoutItem> {
    fn view(&self) -> RenderNode {
        (**self).view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        (**self).on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        (**self).debug_name()
    }
}

impl LayoutItem for Box<dyn LayoutItem> {
    fn layout_node(&self) -> NodeId {
        (**self).layout_node()
    }
}

// pub so the `children!` macro can call it from any crate without naming the module
pub fn box_item(item: impl LayoutItem + 'static) -> Box<dyn LayoutItem> {
    Box::new(item)
}

pub(crate) fn register_container(
    ctx: &mut WidgetCtx,
    layout_style: LayoutStyle,
    children: Vec<Box<dyn LayoutItem>>,
) -> Result<(NodeId, RwSignal<Rect>, TrackedChildren), LayoutError> {
    let child_nodes = children.iter().map(|c| c.layout_node()).collect::<Vec<_>>();
    let node = new_container(ctx, layout_style, &child_nodes)?;
    let rect = track_layout(ctx, node).expect("new_container always registers a signal");
    let children = children
        .into_iter()
        .map(|c| {
            let rect = track_layout(ctx, c.layout_node());
            let item = Rc::new(RefCell::new(c));
            let segment = mount_item_segment(Rc::clone(&item));
            Child {
                item,
                rect,
                segment,
            }
        })
        .collect();
    Ok((node, rect, children))
}

/// Implements `LeafWidget` for a struct that has a `leaf: LayoutLeaf` field.
#[macro_export]
macro_rules! impl_leaf_widget {
    ($struct:ident) => {
        impl $crate::layout_item::LeafWidget for $struct {
            fn layout_leaf(&self) -> &$crate::layout_leaf::LayoutLeaf {
                &self.leaf
            }
        }
    };
}
