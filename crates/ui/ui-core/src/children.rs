//! [`Children`] and [`use_context`]: the content a caller hands a component as a recipe it builds where it places it, and the context a parent provides to what it builds.

use std::any::Any;
use std::rc::Rc;

use layout_core::{LayoutError, NodeId};
use platform_core::Event;
use reactive_core::OwnerId;
use ui_tree::{Component, EventResult, RenderNode};

use crate::layout_item::{Child, LayoutItem, build_owned, make_child};

/// Content a caller hands a component, **not yet built**: the type of every prop that takes markup, the body a component places as `children` and a named place such as `header` alike.
///
/// The difference between the recipe and the list it makes is the whole of what a compound component needs, and it comes from one fact about how a tree is assembled here: a child is an argument, so it is constructed *before* the parent it is passed to. A `Select.Item` that wanted to know which select it belongs to, what is currently chosen, or what to call when it is picked, was asking a question about something that did not exist yet.
///
/// Handed the recipe instead, the parent builds its context first and then runs the recipe inside it, so a child reaches the parent through [`use_context`](crate::use_context) rather than through props threaded down by hand. The recipe is `Fn`, not `FnOnce`, for a second reason that is not theoretical: a dropdown rebuilds its rows every time the panel opens, so the children have to be makeable more than once.
///
/// The children see whatever is in force where the recipe runs — a [`provide_theme`](crate::provide_theme) scope, a context, anything an owner above provides — exactly as a child written inline there would. That is why a `children` placeholder builds its recipe where it stands rather than once up front.
///
/// The default is a recipe for nothing, which is what a component's children prop holds when its caller handed it none.
#[derive(Clone)]
pub struct Children(Rc<Recipe>);

type Recipe = dyn Fn() -> Result<Vec<Box<dyn LayoutItem>>, LayoutError>;

impl Children {
    pub fn new(
        build: impl Fn() -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> + 'static,
    ) -> Self {
        Self(Rc::new(build))
    }

    /// A recipe that builds one item, the same as [`Children::new`] over `Ok(vec![build()?])`.
    pub fn one(build: impl Fn() -> Result<Box<dyn LayoutItem>, LayoutError> + 'static) -> Self {
        Self::new(move || Ok(vec![build()?]))
    }

    /// Builds the children under the current owner, so they see the theme and contexts in force where this is called. What a `children` placeholder runs where it stands, once per placement.
    pub fn build(&self) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
        (self.0)()
    }

    /// Builds the children with `context` visible to every one of them, and to anything they build in turn.
    ///
    /// The scope is nested, so a select inside a select's own row shadows the outer one rather than colliding with it. It does not close when this returns: each child is mounted under the owner that provides it, so the context is still in force when the child draws or one of its handlers fires later.
    pub fn build_with<T: Any + 'static>(
        &self,
        context: T,
    ) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
        within(context, || self.build())
    }
}

impl Default for Children {
    fn default() -> Self {
        Self::new(|| Ok(Vec::new()))
    }
}

/// Runs `build` under a fresh owner that provides `context`, and mounts each child it built under that owner.
fn within<T: Any + 'static>(
    context: T,
    build: impl FnOnce() -> Result<Vec<Box<dyn LayoutItem>>, LayoutError>,
) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    let nodes = crate::context::record_nodes();
    let (built, owner) = build_owned(|| {
        // The owner is fresh, so the only failure is a component providing the same type twice.
        let _ = services_core::provide(context);
        build()
    })?;
    nodes.keep();
    Ok(built
        .into_iter()
        .map(|item| Box::new(InContext::mount(owner, item)) as Box<dyn LayoutItem>)
        .collect())
}

/// A child mounted under the owner that provided its context, rather than under the container it is placed in, so it draws and takes events with that context in force, as a [`provide_theme`](crate::provide_theme) does for its theme. Adds no layout node.
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

/// The nearest enclosing value of type `T` that a parent provided, or `None` outside any such parent.
///
/// The read half of [`Children::build_with`]. `None` is the honest answer for a piece used on its own — an item outside any menu — and a compound component's pieces should say so rather than panicking, since the call site that made the mistake is markup, not Rust.
pub fn use_context<T: Any + Clone + 'static>() -> Option<T> {
    services_core::try_inject::<T>()
}

#[cfg(test)]
#[path = "children_test.rs"]
mod tests;
