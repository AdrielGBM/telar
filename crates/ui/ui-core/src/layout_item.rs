//! [`LayoutItem`]: a component that also owns a layout node, and the clipping and boxing helpers around one.

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::Rc;

use geometry_core::{BorderRadius, Rect};
use layout_core::{LayoutError, LayoutStyle, NodeId};
use layout_reactive::NodeRecording;
use motion_core::Curve;
use platform_core::Event;
use reactive_core::{OwnerId, RwSignal, current_owner, dispose_owner, owner_scope};
use ui_tree::{Component, EventResult, RenderNode, Segment};

use crate::context::{new_container, record_nodes, track_layout};
use crate::disposal::retire;
use crate::input_region::{InputHandle, Placement, gate_admits, withhold};
use crate::layout_leaf::LayoutLeaf;
use crate::layout_transition::{Flip, translated};
use crate::presence::{Mount, Transition};
use crate::surface::{IDENTITY, apply_enter};

/// A container child. The boxed widget is shared (`Rc<RefCell<…>>`) between event dispatch (which borrows it mutably) and its render `segment` (which borrows it immutably to flatten its `view()`) — they never overlap because dispatch is batched. `rect` is the child's layout signal for hit-testing. `Clone` is a cheap handle copy (all fields are `Rc`/signal): a reactive list clones a `Child` to move a reused item to its new position without rebuilding it.
#[derive(Clone)]
pub(crate) struct Child {
    pub(crate) item: Rc<RefCell<Box<dyn LayoutItem>>>,
    pub(crate) rect: Option<RwSignal<Rect>>,
    pub(crate) segment: Rc<Segment>,
    /// Copied at construction rather than read back through `item`. A widget's layout node never changes, and asking the widget for it would borrow a `RefCell` that is already held mutably whenever a reconcile runs from inside one of these children's own event handlers — a row deleting itself, a strip committing a reorder.
    node: layout_core::NodeId,
    /// The owner everything this child's build created belongs to, for the children whose lifetime is their own. `None` for a child handed in already built, whose lifetime is somebody else's.
    owner: Option<OwnerId>,
    /// The owner that was active while this child was built — which is not the same question as [`owner`], and conflating them was a bug.
    ///
    /// [`owner`] answers *what do I dispose*, and only a child with a lifetime of its own has one. This answers *what do I run under*, and every child has one: a static child built inside a component belongs to that component's scope even though it disposes nothing. A handler needs the second, or a static child resolves its ambient reads against the surface root instead of the component it is part of. For a reactive row the two are the same owner, which is why one field looked like enough.
    ///
    /// [`owner`]: Self::owner
    built_under: Option<OwnerId>,
    /// Set on a child that is kept mounted through its exit, by a [`Presence`](crate::Presence) or a transitioned list.
    presented: Option<Rc<Presented>>,
    /// Set on a child that slides to wherever the layout moves it.
    flip: Option<Rc<Flip>>,
}

struct Presented {
    mount: Mount,
    _input: InputHandle,
}

impl Child {
    pub(crate) fn node(&self) -> layout_core::NodeId {
        self.node
    }

    /// Runs `f` under the scope this child was built in, for a handler firing long after that build.
    pub(crate) fn owning<R>(&self, f: impl FnOnce() -> R) -> R {
        reactive_core::with_owner(self.built_under, f)
    }

    pub(crate) fn owned_by(mut self, owner: OwnerId) -> Self {
        self.owner = Some(owner);
        self
    }

    pub(crate) fn mount(&self) -> Option<Mount> {
        self.presented.as_ref().map(|presented| presented.mount)
    }

    /// Gives the child an arrival and departure, under its own owner so they are freed with it. A child that already has one keeps it.
    pub(crate) fn present(&mut self, transition: Transition, entering: bool) -> Mount {
        if let Some(mount) = self.mount() {
            return mount;
        }
        let mount = reactive_core::with_owner(self.owner.or(self.built_under), || {
            Mount::new(transition, entering)
        });
        let mut input = InputHandle::new();
        input.gate(self.node, Some(Rc::new(move || mount.is_leaving())), None);
        input.place(
            self.node,
            Placement::Transform(Rc::new(move || {
                Some(mount.frame_now().0).filter(|matrix| *matrix != IDENTITY)
            })),
        );
        self.presented = Some(Rc::new(Presented {
            mount,
            _input: input,
        }));
        mount
    }

    /// Makes the child slide to wherever the layout moves it, under its own owner so the animation is freed with it. A child that already slides keeps its curve.
    pub(crate) fn animate_layout(&mut self, curve: Curve) {
        if self.flip.is_some() {
            return;
        }
        let node = self.node;
        let flip =
            reactive_core::with_owner(self.owner.or(self.built_under), || Flip::new(node, curve));
        self.flip = Some(Rc::new(flip));
    }

    /// Takes the child out of its parent's layout flow while it stays drawn where it was, or puts it back. Only a sliding child is ever taken out.
    pub(crate) fn set_out_of_flow(&self, out: bool) {
        if let Some(flip) = &self.flip {
            flip.set_out_of_flow(out);
        }
    }

    /// The child's render boundary, drawn where its transitions have it.
    pub(crate) fn boundary(&self) -> RenderNode {
        let boundary = self.segment.boundary();
        let (matrix, opacity) = self.mount().map_or((IDENTITY, 1.0), |mount| mount.frame());
        let matrix = match &self.flip {
            Some(flip) => translated(matrix, flip.offset()),
            None => matrix,
        };
        apply_enter(boundary, matrix, opacity)
    }

    /// The matrix the child is drawn with right now, without subscribing to it.
    pub(crate) fn matrix_now(&self) -> [f32; 6] {
        let matrix = self.mount().map_or(IDENTITY, |mount| mount.frame_now().0);
        match &self.flip {
            Some(flip) => translated(matrix, flip.offset_now()),
            None => matrix,
        }
    }

    /// Hands `event` to the child under its own scope. A child whose node is gated shut — inert, invisible, or leaving — is withheld from it, whoever set the gate, and one arriving or sliding gets the pointer where it is drawn.
    pub(crate) fn deliver(&self, event: &Event) -> EventResult {
        let send = |event: &Event| self.owning(|| self.item.borrow_mut().on_event(event));
        if !gate_admits(self.node) {
            return withhold(event, send);
        }
        let matrix = self.matrix_now();
        match (matrix != IDENTITY)
            .then(|| crate::pointer::transform_pointer(event, matrix))
            .flatten()
        {
            Some(moved) => send(&moved),
            None => send(event),
        }
    }

    pub(crate) fn is_leaving(&self) -> bool {
        self.mount().is_some_and(|mount| mount.is_leaving_now())
    }
}

/// Registers an already-built widget as a container child: tracks its layout rect and mounts its render segment. Used by reactive lists to fold a freshly-built item into the child set (the per-item half of [`register_container`]).
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
        owner: None,
        built_under: current_owner(),
        presented: None,
        flip: None,
    }
}

/// Builds a child that owns its own lifetime — a reactive list's row, an `if` branch — under a fresh owner, so [`dispose_child`] frees everything the build made rather than waiting for the last handle to drop.
///
/// The scope covers `make_child` as well as the build: the rect signal and the render segment's effect are per-item too, and an item that leaves has no more use for either.
pub(crate) fn build_child(
    build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
) -> Result<Child, LayoutError> {
    let (child, owner) = build_owned(|| build().map(make_child))?;
    Ok(child.owned_by(owner))
}

/// Runs `build` under a fresh owner and disposes it on error or panic, so a failed build does not leak one owner's worth of signals and effects per attempt; the nodes it created are freed separately by the [`NodeRecording`] around the build.
pub(crate) fn build_owned<T>(
    build: impl FnOnce() -> Result<T, LayoutError>,
) -> Result<(T, OwnerId), LayoutError> {
    let scope = owner_scope();
    let owner = scope.id();
    let outcome = catch_unwind(AssertUnwindSafe(build));
    drop(scope);
    match outcome {
        Ok(Ok(built)) => Ok((built, owner)),
        Ok(Err(err)) => {
            dispose_owner(owner);
            Err(err)
        }
        Err(payload) => {
            dispose_owner_contained(owner);
            resume_unwind(payload)
        }
    }
}

/// The build's panic is the one worth reporting, so a teardown panicking on the way out is dropped rather than replacing it.
fn dispose_owner_contained(owner: OwnerId) {
    let _ = catch_unwind(AssertUnwindSafe(|| dispose_owner(owner)));
}

/// One pass of building children, released as a whole unless [kept](Self::keep), so a row panicking partway through a pass does not leave the rows built earlier in that pass owned by the host.
#[must_use = "dropping the pass frees what it built"]
pub(crate) struct BuildPass {
    nodes: Option<NodeRecording>,
    owners: Vec<OwnerId>,
}

impl BuildPass {
    pub(crate) fn start() -> Self {
        Self {
            nodes: Some(record_nodes()),
            owners: Vec::new(),
        }
    }

    pub(crate) fn build_child(
        &mut self,
        build: impl FnOnce() -> Result<Box<dyn LayoutItem>, LayoutError>,
    ) -> Result<Child, LayoutError> {
        let child = build_child(build)?;
        self.owners.extend(child.owner);
        Ok(child)
    }

    pub(crate) fn keep(mut self) {
        self.owners.clear();
        if let Some(nodes) = self.nodes.take() {
            nodes.keep();
        }
    }
}

impl Drop for BuildPass {
    fn drop(&mut self) {
        for owner in std::mem::take(&mut self.owners) {
            dispose_owner_contained(owner);
        }
        drop(self.nodes.take());
    }
}

/// Retires a child: what it created, and the layout node it created it on. See [`crate::disposal`] for when.
pub(crate) fn dispose_child(child: &Child) {
    // Retiring frees the node itself, so its owner must not free it again as an orphan.
    child.set_out_of_flow(false);
    retire(child.owner, child.node());
}

pub(crate) type TrackedChildren = Vec<Child>;

/// Mounts a reactive segment that renders a shared boxed item via its `view()`. Uses `try_borrow` so a re-entrant render while the item is mid event-dispatch (mutably borrowed) keeps the previous frame instead of panicking; a later flush re-runs it.
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

/// A component that also owns a layout node, which is what lets it be placed among siblings.
pub trait LayoutItem: Component {
    fn layout_node(&self) -> NodeId;

    fn occludes(&self) -> bool {
        true
    }
}

/// Wraps a child so its rendered output is clipped to the child's own layout rect. When the child collapses to a zero rect (e.g. a section hidden via `display:none`), the clip is empty, so nothing inside draws — even a widget left with a stale rect or one that paints at fixed coordinates. Layout is unchanged: `layout_node` passes through to the wrapped child.
///
/// The pointer stops at the same edge. A press, a move or the wheel landing outside the clip never reaches the subtree, so a widget cut off by the clip cannot take the click that visually belongs to whatever is drawn over it — clipped away is *gone*, not merely invisible. Everything else passes through: a release or a `CursorLeft` is how a widget that was pressed or hovered inside the clip settles again, and swallowing those would leave it stuck in a state the pointer has already left.
pub struct ClippedItem {
    inner: Box<dyn LayoutItem>,
    rect: RwSignal<Rect>,
    clip: Clip,
    _input: InputHandle,
}

/// The shape a [`ClippedItem`] cuts to: which edges do the cutting, how round the corners are, and how far in from the edge the cut sits.
///
/// One value rather than three arguments, and a *shape* rather than an axis, because the renderer's clip node has always taken a radius and the markup had no way to ask for one — so a rounded box with a clipped child cut its corners square, and the only remedy was to not clip.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Clip {
    pub axis: ClipAxis,
    pub radius: BorderRadius,
    /// Pulled in from every cutting edge. What a stroked box wants: cut inside the border rather than under it, so the border stays whole and the content stops at its inner edge.
    pub inset: f32,
    pub pointer: ClipPointer,
}

/// Whether a cut edge stops the pointer as well as the paint.
///
/// [`Stop`](ClipPointer::Stop) is what a viewport wants and the default: clipped away is *gone*, so a widget cut off cannot take the click that visually belongs to whatever is drawn over it. [`Through`](ClipPointer::Through) is for a clip that is only about paint — a rounded frame over a canvas, where a node dragged out past the edge has to keep following the hand, and a clip that swallowed the moves would drop it at the border.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ClipPointer {
    #[default]
    Stop,
    Through,
}

impl Clip {
    /// The node's rect, both ways, square-cornered — a viewport.
    pub fn both() -> Self {
        Self::default()
    }

    /// Its left and right edges; whatever sits above or below is left alone.
    pub fn x() -> Self {
        Self {
            axis: ClipAxis::Horizontal,
            ..Self::default()
        }
    }

    /// Its top and bottom edges; whatever sits left or right of it is left alone.
    pub fn y() -> Self {
        Self {
            axis: ClipAxis::Vertical,
            ..Self::default()
        }
    }

    pub fn rounded(self, radius: impl Into<BorderRadius>) -> Self {
        Self {
            radius: radius.into(),
            ..self
        }
    }

    pub fn inset(self, inset: f32) -> Self {
        Self { inset, ..self }
    }

    /// Cuts what is drawn and nothing else.
    pub fn paint_only(self) -> Self {
        Self {
            pointer: ClipPointer::Through,
            ..self
        }
    }
}

/// Which of a [`Clip`]'s own edges do the cutting.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ClipAxis {
    /// The node's rect, both ways — a viewport.
    #[default]
    Both,
    /// Its left and right edges; whatever sits above or below is left alone.
    Horizontal,
    /// Its top and bottom edges; whatever sits left or right of it is left alone.
    Vertical,
}

/// Half the extent of the free axis of a one-way clip: past any window a platform hands out, and small enough to stay exact in an `f32`, so the axis bounds nothing without being an infinity the renderer has to special-case.
const UNBOUNDED: f32 = 1.0e6;

impl ClippedItem {
    /// A clip cutting to `clip`.
    ///
    /// A one-way clip is what a strip of items wants when it has to stop at its ends but not across its thickness: a tab bar or a toolbar cut where the room runs out, whose items still carry a focus ring, a badge or a shadow past the strip's own edge. CSS cannot express this — one axis set to `hidden` forces the other out of `visible` — so a row that only wanted its ends cut has to clip the overflow it meant to keep.
    pub fn new(inner: Box<dyn LayoutItem>, clip: Clip) -> Self {
        let node = inner.layout_node();
        let rect = track_layout(node).expect("clipped item's node not registered");
        let mut input = InputHandle::new();
        if clip.pointer == ClipPointer::Stop {
            input.place(
                node,
                Placement::Clip(Rc::new(move || Self::cut(rect.peek(), clip))),
            );
        }
        Self {
            inner,
            rect,
            clip,
            _input: input,
        }
    }

    fn cut(rect: Rect, clip: Clip) -> Rect {
        let inset = clip.inset;
        match clip.axis {
            ClipAxis::Both => Rect::new(
                rect.x + inset,
                rect.y + inset,
                (rect.width - inset * 2.0).max(0.0),
                (rect.height - inset * 2.0).max(0.0),
            ),
            ClipAxis::Horizontal => Rect::new(
                rect.x + inset,
                -UNBOUNDED,
                (rect.width - inset * 2.0).max(0.0),
                UNBOUNDED * 2.0,
            ),
            ClipAxis::Vertical => Rect::new(
                -UNBOUNDED,
                rect.y + inset,
                UNBOUNDED * 2.0,
                (rect.height - inset * 2.0).max(0.0),
            ),
        }
    }
}

impl LayoutItem for ClippedItem {
    fn layout_node(&self) -> NodeId {
        self.inner.layout_node()
    }

    fn occludes(&self) -> bool {
        self.inner.occludes()
    }
}

impl Component for ClippedItem {
    fn view(&self) -> RenderNode {
        RenderNode::clip(
            Self::cut(self.rect.get(), self.clip),
            self.clip.radius,
            [self.inner.view()],
        )
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if self.clip.pointer == ClipPointer::Through {
            return self.inner.on_event(event);
        }
        let outside =
            |x: f64, y: f64| !Self::cut(self.rect.get(), self.clip).contains(x as f32, y as f32);
        match event {
            Event::PointerPressed { x, y, .. }
            | Event::PointerMoved { x, y, .. }
            | Event::Scrolled { x, y, .. }
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
/// An `Effect` deregisters on drop, so one bound to a `let` inside a function that returns a widget stops the moment that function returns — the closure runs exactly once and then never again, which reads as a working binding right up until the value it derives is expected to move. A widget that owns its effects (`Container::keeping`) solves that for itself, and this solves it for anything else: a leaf, a boxed component, whatever a `.rsx` happens to have at its root.
///
/// Unrelated to [`kept`](crate::kept), which keeps a *value* across rebuilds of a surface. This keeps a subscription alive for as long as a widget lives.
///
impl<T: LeafWidget + Component> LayoutItem for T {
    fn layout_node(&self) -> NodeId {
        self.layout_leaf().node
    }
}

// Lets an already-boxed child pass back through `box_item`/`children!` without a second wrap, so a transpiled component composes as a `[view]` child.
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

    fn occludes(&self) -> bool {
        (**self).occludes()
    }
}

// `pub` so the `children!` macro can call it from any crate without naming the module.
/// Boxes a widget as a `dyn LayoutItem`, so a heterogeneous child list can hold it.
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
#[path = "layout_item_test.rs"]
mod tests;
