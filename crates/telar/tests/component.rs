//! `#[component]`: a function of named arguments, read as a tag.

use telar::{Children, Container, LayoutError, LayoutItem, LayoutStyle, RwSignal};

/// What every widget a `[view]` cannot build for itself looks like from the markup's side: named props in,
/// one item out. The arguments are the props, and nothing here says so twice.
#[telar::component]
fn probe(
    /// A prop with no attribute is required: forgetting it is a compile error naming it.
    size: f32,
    #[props(default = 3)] times: u8,
    seen: RwSignal<(f32, u8)>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    seen.set((size, times));
    Ok(Box::new(Container::new(
        LayoutStyle::new().width(size),
        vec![],
    )?))
}

/// A component that takes what the call site nested inside it, by naming the one argument that is not a prop.
#[telar::component]
fn wrapper(children: Children, seen: RwSignal<usize>) -> Result<Box<dyn LayoutItem>, LayoutError> {
    seen.set(children.build()?.len());
    Ok(Box::new(Container::new(LayoutStyle::new(), vec![])?))
}

#[test]
fn the_arguments_are_the_props() {
    telar::reset_layout_runtime();
    let seen = telar::signal((0.0, 0u8));
    let _built = probe(
        ProbeProps::props().size(12.0).seen(seen).build(),
        Children::default(),
    )
    .expect("the component builds");
    assert_eq!(
        seen.get(),
        (12.0, 3),
        "the named argument arrived and the one with a default defaulted"
    );
}

#[test]
fn children_are_taken_by_naming_them() {
    telar::reset_layout_runtime();
    let seen = telar::signal(0usize);
    let nested = Container::new(LayoutStyle::new().width(4.0), vec![]).expect("the child builds");
    let nested = std::cell::RefCell::new(Some(Box::new(nested) as Box<dyn LayoutItem>));
    let children = Children::new(move || {
        let mut slots = telar::Slots::new();
        if let Some(item) = nested.borrow_mut().take() {
            slots.push(None, item);
        }
        Ok(slots)
    });
    let _built =
        wrapper(WrapperProps::props().seen(seen).build(), children).expect("the component builds");
    assert_eq!(
        seen.get(),
        1,
        "what the call site nested is what the component was handed"
    );
}
