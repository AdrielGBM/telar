//! `#[component]`: a function of named arguments, read as a tag.

use telar::{Children, Container, LayoutError, LayoutItem, LayoutStyle, RwSignal};

/// What every widget a `[view]` cannot build for itself looks like from the markup's side: named props in, one item out. The arguments are the props, and nothing here says so twice.
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

/// A component that takes what the call site nested inside it, as a prop like any other.
#[telar::component]
fn wrapper(
    #[props(default)] children: Children,
    seen: RwSignal<usize>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    seen.set(children.build()?.len());
    Ok(Box::new(Container::new(LayoutStyle::new(), vec![])?))
}

#[test]
fn the_arguments_are_the_props() {
    telar::reset_layout_runtime();
    let seen = telar::signal((0.0, 0u8));
    let _built =
        probe(ProbeProps::props().size(12.0).seen(seen).build()).expect("the component builds");
    assert_eq!(
        seen.get(),
        (12.0, 3),
        "the named argument arrived and the one with a default defaulted"
    );
}

#[test]
fn children_are_a_prop() {
    telar::reset_layout_runtime();
    let seen = telar::signal(0usize);
    let children = Children::new(|| {
        Ok(vec![
            Box::new(Container::new(LayoutStyle::new().width(4.0), vec![])?) as Box<dyn LayoutItem>,
        ])
    });
    let _built = wrapper(WrapperProps::props().children(children).seen(seen).build())
        .expect("the component builds");
    assert_eq!(
        seen.get(),
        1,
        "what the call site nested is what the component was handed"
    );

    let _built = wrapper(WrapperProps::props().seen(seen).build()).expect("the component builds");
    assert_eq!(
        seen.get(),
        0,
        "and a call site that nested nothing hands over nothing"
    );
}
