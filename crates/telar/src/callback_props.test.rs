//! A component's callback props as Rust sets them: a closure handed to the setter takes its argument types from the `Handler` the prop holds, and a handler held already is handed on with `maybe_`.

use std::cell::Cell;
use std::rc::Rc;

use telar::{Handler, Props};

#[derive(Props)]
struct PickerProps {
    label: &'static str,
    #[props(default = Handler::from(|| {}))]
    on_open: Handler,
    #[props(some, default)]
    on_toggle: Option<Handler<fn(bool)>>,
    #[props(default = Handler::from(|_: usize, _: usize| {}))]
    on_move: Handler<fn(usize, usize)>,
    #[props(some, default)]
    on_resize: Option<Handler<fn(f32, f32)>>,
}

fn run(props: &PickerProps) {
    props.on_open.run();
    if let Some(on_toggle) = &props.on_toggle {
        on_toggle.run(true);
    }
    props.on_move.run(2, 7);
    if let Some(on_resize) = &props.on_resize {
        on_resize.run(3.0, 4.0);
    }
}

fn cell<T: Default>() -> Rc<Cell<T>> {
    Rc::new(Cell::new(T::default()))
}

/// Every closure here calls a method on its arguments, which a closure whose argument types were left to inference could not.
#[test]
fn a_closure_handed_to_a_callback_setter_takes_its_argument_types_from_the_handler() {
    let (opened, toggled, moved, resized) = (cell(), cell(), cell(), cell());
    let props = PickerProps::props()
        .label("Pick")
        .on_open({
            let opened = Rc::clone(&opened);
            move || opened.set(true)
        })
        .on_toggle({
            let toggled = Rc::clone(&toggled);
            move |on| toggled.set(on.then_some("on"))
        })
        .on_move({
            let moved = Rc::clone(&moved);
            move |from, to| moved.set(from.abs_diff(to))
        })
        .on_resize({
            let resized = Rc::clone(&resized);
            move |width, height| resized.set(width.max(height))
        })
        .build();
    run(&props);
    assert!(opened.get());
    assert_eq!(toggled.get(), Some("on"));
    assert_eq!(moved.get(), 5);
    assert_eq!(resized.get(), 4.0);
}

/// What a component does with its own callback props when it calls another: hands each on as it holds it, an `Option` or a `Handler`.
#[test]
fn a_handler_held_already_is_handed_on_with_maybe() {
    let (opened, toggled) = (cell(), cell());
    let outer = PickerProps::props()
        .label("Outer")
        .on_open({
            let opened = Rc::clone(&opened);
            move || opened.set(true)
        })
        .on_toggle({
            let toggled = Rc::clone(&toggled);
            move |on| toggled.set(on)
        })
        .build();
    let inner = PickerProps::props()
        .label("Inner")
        .maybe_on_open(Some(outer.on_open.clone()))
        .maybe_on_toggle(outer.on_toggle.clone())
        .maybe_on_resize(outer.on_resize.clone())
        .build();
    run(&inner);
    assert!(opened.get(), "the plain prop runs the outer handler");
    assert!(toggled.get(), "and so does the optional one");
    assert!(
        inner.on_resize.is_none(),
        "an optional callback the outer props left unset stays unset"
    );
}

#[test]
fn handing_on_none_leaves_the_prop_as_it_was() {
    let opened = cell();
    let props = PickerProps::props()
        .label("Pick")
        .on_open({
            let opened = Rc::clone(&opened);
            move || opened.set(true)
        })
        .maybe_on_open(None)
        .on_toggle(|_| {})
        .maybe_on_toggle(None)
        .build();
    run(&props);
    assert!(opened.get(), "the closure set before is still there");
    assert!(props.on_toggle.is_some());
    assert!(
        PickerProps::props()
            .label("Bare")
            .maybe_on_toggle(None)
            .build()
            .on_toggle
            .is_none(),
        "and an optional callback never set stays unset"
    );
}

/// A `.rsx` call hands a value it names to `__forward_`, not knowing whether the name holds a closure, a `Handler` or an `Option` of one.
#[test]
fn a_named_value_reaches_a_callback_prop_whatever_it_holds() {
    let toggles = cell::<u32>();
    let count = {
        let toggles = Rc::clone(&toggles);
        move |_: bool| toggles.set(toggles.get() + 1)
    };
    let handler: Handler<fn(bool)> = Handler::from(count.clone());
    let given: Option<Handler<fn(bool)>> = Some(handler.clone());
    let unset: Option<Handler<fn(bool)>> = None;
    let moves = cell::<usize>();
    let on_move: Handler<fn(usize, usize)> = {
        let moves = Rc::clone(&moves);
        Handler::from(move |from, to| moves.set(from + to))
    };

    for props in [
        PickerProps::props().__forward_on_toggle(count),
        PickerProps::props().__forward_on_toggle(handler),
        PickerProps::props().__forward_on_toggle(given),
    ] {
        run(&props
            .__forward_on_move(on_move.clone())
            .__forward_label("Pick")
            .build());
    }
    let left = PickerProps::props()
        .__forward_on_toggle(unset)
        .__forward_label("Pick")
        .build();

    assert_eq!(toggles.get(), 3, "each of the three ran the closure");
    assert_eq!(moves.get(), 9);
    assert!(left.on_toggle.is_none(), "a `None` sets nothing");
}

/// A `.rsx` call hands a handler it works out to `__follow_`, which takes it once.
#[test]
fn a_worked_out_handler_reaches_a_callback_prop() {
    telar::reset_runtime();
    let toggled = cell();
    let handler: Handler<fn(bool)> = {
        let toggled = Rc::clone(&toggled);
        Handler::from(move |on| toggled.set(on))
    };
    let props = PickerProps::props()
        .label("Pick")
        .__follow_on_toggle(move || Some(handler.clone()))
        .build();
    run(&props);
    assert!(toggled.get());
}
