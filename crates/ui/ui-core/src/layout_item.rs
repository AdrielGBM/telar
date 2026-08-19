use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{LayoutError, LayoutStyle, NodeId};
use platform_core::Event;
use reactive_core::RwSignal;
use ui_tree::{Component, EventResult, RenderNode, Segment};

use crate::context::{new_container, track_layout};
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
    /// Copied at construction rather than read back through `item`. A widget's layout node never changes, and
    /// asking the widget for it would borrow a `RefCell` that is already held mutably whenever a reconcile
    /// runs from inside one of these children's own event handlers — a row deleting itself, a strip
    /// committing a reorder.
    node: layout_core::NodeId,
}

impl Child {
    pub(crate) fn node(&self) -> layout_core::NodeId {
        self.node
    }
}

/// Registers an already-built widget as a container child: tracks its layout rect and mounts its render
/// segment. Used by reactive lists to fold a freshly-built item into the child set (the per-item half of
/// [`register_container`]).
pub(crate) fn make_child(widget: Box<dyn LayoutItem>) -> Child {
    let node = widget.layout_node();
    let rect = track_layout(node);
    let item = Rc::new(RefCell::new(widget));
    let segment = mount_item_segment(Rc::clone(&item));
    Child {
        item,
        rect,
        segment,
        node,
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

    /// Whether this widget stands in front of whatever its siblings drew underneath it, for a pointer event
    /// its parent is hit-testing.
    ///
    /// True for anything that occupies its box, which is everything that draws: the topmost child under the
    /// pointer takes the event whether or not it wants it, exactly as a browser hit-tests — otherwise a
    /// floating panel lets the wheel through to the pane it covers. The one thing that is not there for this
    /// purpose is an [`Overlay`](crate::Overlay): the registry routes positioned events to it *before* the
    /// tree walk, so its in-tree node must not shadow the siblings it was portaled away from.
    fn pointer_opaque(&self) -> bool {
        true
    }
}

/// Wraps a child so its rendered output is clipped to the child's own layout rect. When the child
/// collapses to a zero rect (e.g. a section hidden via `display:none`), the clip is empty, so nothing
/// inside draws — even a widget left with a stale rect or one that paints at fixed coordinates. Layout
/// is unchanged: `layout_node` passes through to the wrapped child.
///
/// The pointer stops at the same edge. A press or a move landing outside the clip never reaches the subtree,
/// so a widget cut off by the clip cannot take the click that visually belongs to whatever is drawn over it —
/// clipped away is *gone*, not merely invisible. Everything else passes through: a release or a `CursorLeft`
/// is how a widget that was pressed or hovered inside the clip settles again, and swallowing those would leave
/// it stuck in a state the pointer has already left.
pub struct ClippedItem {
    inner: Box<dyn LayoutItem>,
    rect: RwSignal<Rect>,
    axis: ClipAxis,
}

/// Which of a [`ClippedItem`]'s own edges do the cutting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClipAxis {
    /// The node's rect, both ways — a viewport.
    Both,
    /// Its left and right edges; whatever sits above or below is left alone.
    Horizontal,
    /// Its top and bottom edges; whatever sits left or right of it is left alone.
    Vertical,
}

/// Half the extent of the free axis of a one-way clip: past any window a platform hands out, and small enough
/// to stay exact in an `f32`, so the axis bounds nothing without being an infinity the renderer has to
/// special-case.
const UNBOUNDED: f32 = 1.0e6;

impl ClippedItem {
    pub fn new(inner: Box<dyn LayoutItem>) -> Self {
        Self::along(inner, ClipAxis::Both)
    }

    /// A clip that cuts along `axis` only, leaving the other free.
    ///
    /// What a strip of items wants when it has to stop at its ends but not across its thickness: a tab bar or a
    /// toolbar cut where the room runs out, whose items still carry a focus ring, a badge or a shadow past the
    /// strip's own edge. CSS cannot express this — one axis set to `hidden` forces the other out of `visible` —
    /// so a row that only wanted its ends cut has to clip the overflow it meant to keep.
    pub fn along(inner: Box<dyn LayoutItem>, axis: ClipAxis) -> Self {
        let rect = track_layout(inner.layout_node()).expect("clipped item's node not registered");
        Self { inner, rect, axis }
    }

    fn clip(&self) -> Rect {
        let rect = self.rect.get();
        match self.axis {
            ClipAxis::Both => rect,
            ClipAxis::Horizontal => Rect::new(rect.x, -UNBOUNDED, rect.width, UNBOUNDED * 2.0),
            ClipAxis::Vertical => Rect::new(-UNBOUNDED, rect.y, UNBOUNDED * 2.0, rect.height),
        }
    }
}

impl LayoutItem for ClippedItem {
    fn layout_node(&self) -> NodeId {
        self.inner.layout_node()
    }

    fn pointer_opaque(&self) -> bool {
        self.inner.pointer_opaque()
    }
}

impl Component for ClippedItem {
    fn view(&self) -> RenderNode {
        RenderNode::clip(
            self.clip(),
            renderer_core::BorderRadius::zero(),
            [self.inner.view()],
        )
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        let outside = |x: f64, y: f64| !self.clip().contains(x as f32, y as f32);
        match event {
            Event::PointerPressed { x, y, .. } | Event::PointerMoved { x, y, .. }
                if outside(*x, *y) =>
            {
                EventResult::Ignored
            }
            _ => self.inner.on_event(event),
        }
    }

    fn debug_name(&self) -> &'static str {
        "Clipped"
    }
}

/// Wraps a widget so that dropping the widget drops a set of [`Effect`](reactive_core::Effect)s with it.
///
/// An `Effect` deregisters on drop, so one bound to a `let` inside a function that returns a widget stops
/// the moment that function returns — the closure runs exactly once and then never again, which reads as a
/// working binding right up until the value it derives is expected to move. A widget that owns its effects
/// ([`Container::keeping`](crate::Container::keeping)) solves that for itself, and this solves it for
/// anything else: a leaf, a boxed component, whatever a `.rsx` happens to have at its root.
///
/// Unrelated to [`kept`](crate::kept), which keeps a *value* across rebuilds of a surface. This keeps a
/// subscription alive for as long as a widget lives.
///
/// Invisible in every other respect. Layout, painting, hit-testing and `debug_name` all pass straight
/// through, so wrapping costs no layout node and the devtools tree still names the widget underneath.
pub struct Holding {
    inner: Box<dyn LayoutItem>,
    _effects: Vec<reactive_core::Effect>,
}

impl Holding {
    pub fn new(inner: Box<dyn LayoutItem>, effects: Vec<reactive_core::Effect>) -> Self {
        Self {
            inner,
            _effects: effects,
        }
    }
}

impl LayoutItem for Holding {
    fn layout_node(&self) -> NodeId {
        self.inner.layout_node()
    }

    fn pointer_opaque(&self) -> bool {
        self.inner.pointer_opaque()
    }
}

impl Component for Holding {
    fn view(&self) -> RenderNode {
        self.inner.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.inner.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        self.inner.debug_name()
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

    fn pointer_opaque(&self) -> bool {
        (**self).pointer_opaque()
    }
}

// pub so the `children!` macro can call it from any crate without naming the module
pub fn box_item(item: impl LayoutItem + 'static) -> Box<dyn LayoutItem> {
    Box::new(item)
}

pub(crate) fn register_container(
    layout_style: LayoutStyle,
    children: Vec<Box<dyn LayoutItem>>,
) -> Result<(NodeId, RwSignal<Rect>, TrackedChildren), LayoutError> {
    let child_nodes = children.iter().map(|c| c.layout_node()).collect::<Vec<_>>();
    let node = new_container(layout_style, &child_nodes)?;
    let rect = track_layout(node).expect("new_container always registers a signal");
    let children = children.into_iter().map(make_child).collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StyledContainer;
    use crate::context::{compute_layout, reset_layout_runtime};
    use layout_core::AvailableSpace;
    use platform_core::{PointerButton, PointerSource};
    use renderer_core::RectStyle;
    use std::cell::Cell;
    use std::rc::Rc;

    fn press(x: f64, y: f64) -> Event {
        Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    fn release(x: f64, y: f64) -> Event {
        Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        }
    }

    /// A press outside the clip does not reach the subtree, and one inside it still does.
    ///
    /// The case this exists for: a row of items wider than the box it is clipped to. The overflow is not drawn,
    /// so whatever is painted over that strip looks like the only thing there — and a hidden item that still
    /// answered a click there would be stealing it from the visible one.
    #[test]
    fn a_press_outside_the_clip_never_reaches_what_it_hides() {
        let pressed = Rc::new(Cell::new(false));
        let sink = Rc::clone(&pressed);
        reset_layout_runtime();
        let inner = StyledContainer::new(
            LayoutStyle::new().flex_row().width(100.0).height(20.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || sink.set(true));
        // The clip is the child's own rect, so it is cut to a 40px window by laying it out in one.
        let mut clipped = ClippedItem::new(Box::new(
            StyledContainer::new(
                LayoutStyle::new().flex_row().width(40.0).height(20.0),
                |_r| RectStyle::default(),
                vec![Box::new(inner)],
            )
            .unwrap(),
        ));
        compute_layout(
            clipped.layout_node(),
            AvailableSpace::Definite(40.0),
            AvailableSpace::Definite(20.0),
        )
        .unwrap();

        clipped.on_event(&press(80.0, 10.0));
        clipped.on_event(&release(80.0, 10.0));
        assert!(
            !pressed.get(),
            "a tap 40px past the clip's edge reached the item hidden behind it"
        );

        clipped.on_event(&press(10.0, 10.0));
        clipped.on_event(&release(10.0, 10.0));
        assert!(
            pressed.get(),
            "and a tap on the part that is actually drawn still has to land"
        );
    }
}
