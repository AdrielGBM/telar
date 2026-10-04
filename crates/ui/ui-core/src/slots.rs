//! [`Slots`]: the children a call site nested inside a component, routed to the placeholders that ask for them.

use std::any::Any;
use std::rc::Rc;

use layout_core::{LayoutError, NodeId};
use platform_core::Event;
use reactive_core::OwnerId;
use ui_tree::{Component, EventResult, RenderNode};

use crate::layout_item::{Child, LayoutItem, build_owned, make_child};

/// The children a component receives from its call site, grouped by slot. A bare child lands in the default slot (`None`); a child written with `slot:"name"` lands in that named slot. [`take_default`](Self::take_default) and [`take`](Self::take) drain one slot in call-site order, and draining is one-shot: taking the same slot twice yields an empty list the second time.
#[derive(Default)]
pub struct Slots {
    items: Vec<(Option<&'static str>, Box<dyn LayoutItem>)>,
}

impl Slots {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, name: Option<&'static str>, item: Box<dyn LayoutItem>) {
        self.items.push((name, item));
    }

    /// Appends `items` as default (unnamed) children. What every generated component call site does with the children it collected, so the emitter writes one call instead of a hand-rolled loop 418 times over.
    pub fn extend_default(&mut self, items: impl IntoIterator<Item = Box<dyn LayoutItem>>) {
        self.items
            .extend(items.into_iter().map(|item| (None, item)));
    }

    /// How many children are still undrained, across every slot.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the call site passed no children at all — what a compound component checks before falling back to whatever it shows when it was given nothing.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Drains the default (unnamed) children in call-site order.
    pub fn take_default(&mut self) -> Vec<Box<dyn LayoutItem>> {
        self.take_matching(|n| n.is_none())
    }

    /// Drains the children assigned to the named slot `name`, in call-site order.
    pub fn take(&mut self, name: &str) -> Vec<Box<dyn LayoutItem>> {
        self.take_matching(|n| *n == Some(name))
    }

    /// Drains the children `request` asks for into a `Slots` of their own, keeping their slot names.
    pub fn take_requested(&mut self, request: SlotRequest<'_>) -> Slots {
        let (items, rest) = std::mem::take(&mut self.items)
            .into_iter()
            .partition(|(name, _)| request.includes(*name));
        self.items = rest;
        Slots { items }
    }

    fn take_matching(
        &mut self,
        pred: impl Fn(&Option<&'static str>) -> bool,
    ) -> Vec<Box<dyn LayoutItem>> {
        let mut taken = Vec::new();
        let mut rest = Vec::new();
        for (name, item) in std::mem::take(&mut self.items) {
            if pred(&name) {
                taken.push(item);
            } else {
                rest.push((name, item));
            }
        }
        self.items = rest;
        taken
    }
}

/// Which slots one run of a [`Children`] recipe is asked to build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotRequest<'a> {
    /// Every slot, which is what [`Children::build`] asks for.
    All,
    /// One slot, `None` naming the default one: what a `children` placeholder asks for where it stands.
    Only(Option<&'a str>),
}

impl SlotRequest<'_> {
    /// Whether a child routed to `slot` belongs in this build.
    pub fn includes(self, slot: Option<&str>) -> bool {
        match self {
            Self::All => true,
            Self::Only(wanted) => wanted == slot,
        }
    }
}

/// A component's markup children, **not yet built**.
///
/// [`Slots`] is the list a call site already made; this is the recipe for making it. The difference is the whole of what a compound component needs, and it comes from one fact about how a tree is assembled here: a child is an argument, so it is constructed *before* the parent it is passed to. A `Select.Item` that wanted to know which select it belongs to, what is currently chosen, or what to call when it is picked, was asking a question about something that did not exist yet.
///
/// Handed the recipe instead, the parent builds its context first and then runs the recipe inside it, so a child reaches the parent through [`use_context`](crate::use_context) rather than through props threaded down by hand. The recipe is `Fn`, not `FnOnce`, for a second reason that is not theoretical: a dropdown rebuilds its rows every time the panel opens, so the children have to be makeable more than once.
///
/// The children see whatever is in force where the recipe runs — a [`provide_theme`](crate::provide_theme) scope, a context, anything an owner above provides — exactly as a child written inline there would. That is why a `children` placeholder builds its slot where it stands ([`build_slot`](Self::build_slot)) rather than once up front.
#[derive(Clone)]
pub struct Children(Rc<Recipe>);

type Recipe = dyn Fn(SlotRequest<'_>) -> Result<Slots, LayoutError>;

impl Children {
    /// A recipe that builds every slot on each run. A component that places its slots separately then builds the slots it did not ask for and frees them, which is why a call site in markup hands over [`per_slot`](Self::per_slot) instead.
    pub fn new(build: impl Fn() -> Result<Slots, LayoutError> + 'static) -> Self {
        Self(Rc::new(move |_| build()))
    }

    /// A recipe that builds only the slots each run asks for: what a call site in markup hands over, so each slot can be built inside the scopes of the node it is placed in.
    pub fn per_slot(
        build: impl Fn(SlotRequest<'_>) -> Result<Slots, LayoutError> + 'static,
    ) -> Self {
        Self(Rc::new(build))
    }

    /// Builds the children with `context` visible to every one of them, and to anything they build in turn.
    ///
    /// The scope is nested, so a select inside a select's own row shadows the outer one rather than colliding with it. It does not close when this returns: each child is mounted under the owner that provides it, so the context is still in force when the child draws or one of its handlers fires later.
    pub fn build_with<T: Any + 'static>(&self, context: T) -> Result<Slots, LayoutError> {
        within(context, || (self.0)(SlotRequest::All))
    }

    /// Builds the children with no context of their own, for a component that has nothing to tell them.
    pub fn build(&self) -> Result<Slots, LayoutError> {
        (self.0)(SlotRequest::All)
    }

    /// Builds one slot (`None` for the default one) under the current owner, so its children see the theme and contexts in force where this is called. What a `children` placeholder runs where it stands, once per placement.
    pub fn build_slot(&self, slot: Option<&str>) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
        self.requested(slot).map(|built| only(slot, built))
    }

    /// [`build_slot`](Self::build_slot) with `context` visible to the slot's children, as [`build_with`](Self::build_with) provides it.
    pub fn build_slot_with<T: Any + 'static>(
        &self,
        slot: Option<&str>,
        context: T,
    ) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
        within(context, || self.requested(slot)).map(|built| only(slot, built))
    }

    /// Runs the recipe for `slot`, freeing whatever a recipe that ignores the request built for the other slots.
    fn requested(&self, slot: Option<&str>) -> Result<Slots, LayoutError> {
        let request = SlotRequest::Only(slot);
        let mut built = (self.0)(request)?;
        let wanted = built.take_requested(request);
        for (_, unplaced) in built.items {
            crate::context::remove_node(unplaced.layout_node());
        }
        Ok(wanted)
    }
}

/// The children routed to `slot`.
fn only(slot: Option<&str>, mut built: Slots) -> Vec<Box<dyn LayoutItem>> {
    match slot {
        None => built.take_default(),
        Some(name) => built.take(name),
    }
}

/// Runs `build` under a fresh owner that provides `context`, and mounts each child it built under that owner.
fn within<T: Any + 'static>(
    context: T,
    build: impl FnOnce() -> Result<Slots, LayoutError>,
) -> Result<Slots, LayoutError> {
    let nodes = crate::context::record_nodes();
    let (built, owner) = build_owned(|| {
        // The owner is fresh, so the only failure is a component providing the same type twice.
        let _ = services_core::provide(context);
        build()
    })?;
    nodes.keep();
    let items = built
        .items
        .into_iter()
        .map(|(slot, item)| {
            (
                slot,
                Box::new(InContext::mount(owner, item)) as Box<dyn LayoutItem>,
            )
        })
        .collect();
    Ok(Slots { items })
}

/// A child mounted under the owner that provided its context, rather than under the container it is placed in, so it draws and takes events with that context in force, as a [`ThemeProvider`](crate::ThemeProvider) does for its theme. Adds no layout node.
struct InContext {
    child: Child,
}

impl InContext {
    fn mount(owner: OwnerId, item: Box<dyn LayoutItem>) -> Self {
        Self {
            child: reactive_core::with_owner(Some(owner), || make_child(item)),
        }
    }
}

impl LayoutItem for InContext {
    fn layout_node(&self) -> NodeId {
        self.child.node()
    }

    fn occludes(&self) -> bool {
        self.child
            .item
            .try_borrow()
            .map(|item| item.occludes())
            .unwrap_or(true)
    }
}

impl Component for InContext {
    fn view(&self) -> RenderNode {
        self.child.boundary()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.child.deliver(event)
    }

    fn debug_name(&self) -> &'static str {
        "InContext"
    }
}

impl Default for Children {
    fn default() -> Self {
        Self::new(|| Ok(Slots::new()))
    }
}

impl From<Slots> for Children {
    /// For a caller holding children it already built — a test, or a component forwarding what it was given. Each slot is handed out once and is empty on any later request for it, since a built widget cannot be made twice.
    fn from(slots: Slots) -> Self {
        let cell = std::cell::RefCell::new(slots);
        Self::per_slot(move |request| Ok(cell.borrow_mut().take_requested(request)))
    }
}

/// The nearest enclosing value of type `T` that a parent provided, or `None` outside any such parent.
///
/// The read half of [`Children::build_with`]. `None` is the honest answer for a piece used on its own — an item outside any menu — and a compound component's pieces should say so rather than panicking, since the call site that made the mistake is markup, not Rust.
pub fn use_context<T: Any + Clone + 'static>() -> Option<T> {
    services_core::try_inject::<T>()
}

#[cfg(test)]
#[path = "slots_test.rs"]
mod tests;
