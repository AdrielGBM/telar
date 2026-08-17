use std::any::Any;
use std::rc::Rc;

use layout_core::LayoutError;

use crate::layout_item::LayoutItem;

/// The children a component receives from its call site, grouped by slot. A bare child lands in the
/// default slot (`None`); a child written with `slot:"name"` lands in that named slot. Inside the
/// component, the `children` placeholder drains the default slot and `children name:"x"` drains the
/// `"x"` slot — each in call-site order. Draining is one-shot: a slot placeholder consumes its
/// children, so referencing the same slot twice yields an empty list the second time.
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

    /// Appends `items` as default (unnamed) children. What every generated component call site does with the
    /// children it collected, so the emitter writes one call instead of a hand-rolled loop 418 times over.
    pub fn extend_default(&mut self, items: impl IntoIterator<Item = Box<dyn LayoutItem>>) {
        self.items
            .extend(items.into_iter().map(|item| (None, item)));
    }

    /// How many children are still undrained, across every slot.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the call site passed no children at all — what a compound component checks before falling back
    /// to whatever it shows when it was given nothing.
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

/// A component's markup children, **not yet built**.
///
/// [`Slots`] is the list a call site already made; this is the recipe for making it. The difference is the
/// whole of what a compound component needs, and it comes from one fact about how a tree is assembled here:
/// a child is an argument, so it is constructed *before* the parent it is passed to. A `Select.Item` that
/// wanted to know which select it belongs to, what is currently chosen, or what to call when it is picked,
/// was asking a question about something that did not exist yet.
///
/// Handed the recipe instead, the parent builds its context first and then runs the recipe inside it, so a
/// child reaches the parent through [`use_context`](crate::use_context) rather than through props threaded
/// down by hand. The recipe is `Fn`, not `FnOnce`, for a second reason that is not theoretical: a dropdown
/// rebuilds its rows every time the panel opens, so the children have to be makeable more than once.
#[derive(Clone)]
pub struct Children(Rc<dyn Fn() -> Result<Slots, LayoutError>>);

impl Children {
    pub fn new(build: impl Fn() -> Result<Slots, LayoutError> + 'static) -> Self {
        Self(Rc::new(build))
    }

    /// Builds the children with `context` visible to every one of them, and to anything they build in turn.
    ///
    /// The scope is nested, so a select inside a select's own row shadows the outer one rather than
    /// colliding with it, and it closes when this returns — a widget built afterwards sees nothing.
    pub fn build_with<T: Any + 'static>(&self, context: T) -> Result<Slots, LayoutError> {
        services_core::Scope::with(|| {
            // The scope is fresh, so the only way this fails is a caller providing the same type twice into
            // one scope, which is a bug in the component rather than anything its call site can cause.
            let _ = services_core::provide(context);
            (self.0)()
        })
    }

    /// Builds the children with no context of their own, for a component that has nothing to tell them.
    pub fn build(&self) -> Result<Slots, LayoutError> {
        (self.0)()
    }
}

impl Default for Children {
    fn default() -> Self {
        Self::new(|| Ok(Slots::new()))
    }
}

impl From<Slots> for Children {
    /// For a caller holding children it already built — a test, or a component forwarding what it was given.
    /// The recipe hands them out once and is empty on any later call, since a built widget cannot be made twice.
    fn from(slots: Slots) -> Self {
        let cell = std::cell::RefCell::new(Some(slots));
        Self::new(move || Ok(cell.borrow_mut().take().unwrap_or_default()))
    }
}

/// The nearest enclosing value of type `T` that a parent provided, or `None` outside any such parent.
///
/// The read half of [`Children::build_with`]. `None` is the honest answer for a piece used on its own — an
/// item outside any menu — and a compound component's pieces should say so rather than panicking, since the
/// call site that made the mistake is markup, not Rust.
pub fn use_context<T: Any + Clone + 'static>() -> Option<T> {
    services_core::try_inject::<T>()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use layout_core::LayoutStyle;

    use super::*;
    use crate::container::Container;
    use crate::context::reset_layout_runtime;
    use crate::layout_item::box_item;

    #[derive(Clone)]
    struct Menu(&'static str);

    /// A child records what it could see of its parent while it was being built.
    fn spy(seen: Rc<RefCell<Vec<Option<&'static str>>>>) -> Children {
        Children::new(move || {
            seen.borrow_mut().push(use_context::<Menu>().map(|m| m.0));
            let mut slots = Slots::new();
            slots.push(None, box_item(Container::new(LayoutStyle::new(), vec![])?));
            Ok(slots)
        })
    }

    /// The inversion the whole type exists for. A child is an argument, so it is normally constructed before
    /// its parent — build it from a recipe instead and the parent gets to exist first.
    #[test]
    fn a_child_built_from_the_recipe_can_see_the_parent_making_it() {
        reset_layout_runtime();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let children = spy(seen.clone());

        let slots = children.build_with(Menu("edit")).unwrap();
        assert_eq!(*seen.borrow(), vec![Some("edit")]);
        assert_eq!(slots.len(), 1, "and it is still a child, not just a reader");
    }

    /// A piece used on its own gets `None` rather than a panic: the mistake was made in markup, and a
    /// component that dies on it reports it as a crash in Rust nobody wrote.
    #[test]
    fn a_child_outside_any_parent_sees_nothing() {
        reset_layout_runtime();
        let seen = Rc::new(RefCell::new(Vec::new()));
        spy(seen.clone()).build().unwrap();
        assert_eq!(*seen.borrow(), vec![None]);
    }

    /// The context closes with the build. A widget made afterwards is not inside that menu, and must not
    /// find it lying around.
    #[test]
    fn the_context_does_not_outlive_the_build_that_opened_it() {
        reset_layout_runtime();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let children = spy(seen.clone());
        children.build_with(Menu("edit")).unwrap();
        assert_eq!(use_context::<Menu>().map(|m| m.0), None);
    }

    /// Nesting is what a submenu is, and the inner one has to win inside itself without disturbing the outer.
    #[test]
    fn a_nested_parent_shadows_the_one_it_sits_in() {
        reset_layout_runtime();
        let inner_seen = Rc::new(RefCell::new(Vec::new()));
        let inner = spy(inner_seen.clone());
        let outer_seen = Rc::new(RefCell::new(Vec::new()));
        let outer = {
            let outer_seen = outer_seen.clone();
            Children::new(move || {
                outer_seen
                    .borrow_mut()
                    .push(use_context::<Menu>().map(|m| m.0));
                inner.build_with(Menu("submenu"))?;
                // Read again after the inner scope closed, which is where a stack that popped wrongly shows.
                outer_seen
                    .borrow_mut()
                    .push(use_context::<Menu>().map(|m| m.0));
                Ok(Slots::new())
            })
        };

        outer.build_with(Menu("edit")).unwrap();
        assert_eq!(*inner_seen.borrow(), vec![Some("submenu")]);
        assert_eq!(*outer_seen.borrow(), vec![Some("edit"), Some("edit")]);
    }

    /// The reason the recipe is `Fn` and not `FnOnce`: a dropdown throws its rows away and remakes them every
    /// time the panel opens, so children that could only be built once would come back empty on the second open.
    #[test]
    fn the_recipe_can_be_run_more_than_once() {
        reset_layout_runtime();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let children = spy(seen.clone());

        for _ in 0..3 {
            assert_eq!(children.build_with(Menu("edit")).unwrap().len(), 1);
        }
        assert_eq!(seen.borrow().len(), 3, "a fresh set of rows each time");
    }
}
