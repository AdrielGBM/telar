use telar::testing::{hold, key_with, lay_out, moved, named, press, release, route};
use telar::{
    ComponentList, Key, NamedKey, RectStyle, RwSignal, StyledContainer, Transaction, signal,
};

use super::*;
use crate::test_support::fresh_layout_runtime;

struct Canvas {
    tree: ComponentList,
    value: RwSignal<f32>,
    clamped: RwSignal<bool>,
}

/// A 200x200 canvas holding a handle that sits at `(value, 50)` and reads its value off the pointer's x, clamped to `0..=100`.
fn canvas(start: f32) -> Canvas {
    canvas_reporting(start, Ends::default())
}

/// What a canvas's handle was told about its drags, in order.
type Told = Rc<std::cell::RefCell<Vec<&'static str>>>;

#[derive(Default)]
struct Ends {
    on_start: Option<Rc<dyn Fn()>>,
    on_end: Option<Rc<dyn Fn(bool)>>,
}

impl Ends {
    fn told(told: &Told) -> Self {
        let (started, ended) = (told.clone(), told.clone());
        Self {
            on_start: Some(Rc::new(move || started.borrow_mut().push("start"))),
            on_end: Some(Rc::new(move |kept| {
                ended
                    .borrow_mut()
                    .push(if kept { "kept" } else { "undone" })
            })),
        }
    }
}

/// [`canvas`] with its handle telling `ends` as its drags start and end.
fn canvas_reporting(start: f32, ends: Ends) -> Canvas {
    fresh_layout_runtime();
    ui_core::reset_keyboard();
    telar::focus::clear();
    let value = signal(start);
    let clamped = signal(false);
    let mut props = HandleProps::props()
        .value(value)
        .to_value(Rc::new(|x, _| x))
        .to_point(Rc::new(|value| (value, 50.0)))
        .min(0.0)
        .max(100.0)
        .clamped(clamped)
        .build();
    props.on_start = ends.on_start;
    props.on_end = ends.on_end;
    let dot = handle(props, Children::default()).unwrap();
    let area = StyledContainer::new(
        LayoutStyle::new().width(200.0).height(200.0),
        |_| RectStyle::default(),
        vec![dot],
    )
    .unwrap();
    let node = area.layout_node();
    let tree = ComponentList::new(box_item(area));
    lay_out(node, 200.0, 200.0);
    Canvas {
        tree,
        value,
        clamped,
    }
}

impl Canvas {
    fn event(&mut self, event: telar::Event) {
        route(&mut self.tree, &event);
    }
}

#[test]
fn a_handle_follows_the_pointer_from_where_it_was_grabbed() {
    let mut canvas = canvas(20.0);
    canvas.event(press(22.0, 51.0));
    assert_eq!(canvas.value.peek(), 20.0, "the press itself moved nothing");
    canvas.event(moved(62.0, 51.0));
    assert_eq!(canvas.value.peek(), 60.0);
    canvas.event(moved(42.0, 90.0));
    assert_eq!(canvas.value.peek(), 40.0);
    canvas.event(release(42.0, 90.0));
    assert_eq!(canvas.value.peek(), 40.0);
}

#[test]
fn a_handle_clamps_and_shows_it_while_the_request_is_out_of_range() {
    let mut canvas = canvas(20.0);
    canvas.event(press(20.0, 50.0));
    canvas.event(moved(500.0, 50.0));
    assert_eq!(canvas.value.peek(), 100.0);
    assert!(canvas.clamped.peek(), "pulled past the bound");

    canvas.event(moved(50.0, 50.0));
    assert_eq!(canvas.value.peek(), 50.0);
    assert!(!canvas.clamped.peek(), "back in range");

    canvas.event(moved(-300.0, 50.0));
    assert_eq!(canvas.value.peek(), 0.0);
    assert!(canvas.clamped.peek());
    canvas.event(release(-300.0, 50.0));
    assert!(!canvas.clamped.peek(), "the drag ended");
}

#[test]
fn escape_mid_drag_puts_the_handle_back() {
    let mut canvas = canvas(20.0);
    canvas.event(press(20.0, 50.0));
    canvas.event(moved(80.0, 50.0));
    canvas.event(named(NamedKey::Escape));
    assert_eq!(canvas.value.peek(), 20.0);
    canvas.event(release(80.0, 50.0));
    assert_eq!(canvas.value.peek(), 20.0);
}

#[test]
fn a_focused_handle_is_nudged_by_the_arrow_keys() {
    let mut canvas = canvas(20.0);
    canvas.event(press(20.0, 50.0));
    canvas.event(release(20.0, 50.0));

    canvas.event(named(NamedKey::ArrowRight));
    assert_eq!(canvas.value.peek(), 21.0);
    let shift = hold(true, false);
    canvas.event(key_with(Key::Named(NamedKey::ArrowLeft), shift));
    assert_eq!(canvas.value.peek(), 11.0);
    hold(false, false);
    canvas.value.set(99.5);
    canvas.event(named(NamedKey::ArrowUp));
    assert_eq!(canvas.value.peek(), 100.0);
    assert!(canvas.clamped.peek(), "a nudge past the bound shows it too");
}

#[test]
fn a_handle_joins_a_popover_transaction_on_the_same_value() {
    fresh_layout_runtime();
    let value = signal(10.0);
    let popover = Transaction::new(value);
    let dot = handle(
        HandleProps::props()
            .transaction(popover)
            .to_point(Rc::new(|value| (value, 50.0)))
            .build(),
        Children::default(),
    )
    .unwrap();
    let node = dot.layout_node();
    let mut tree = ComponentList::new(dot);
    lay_out(node, 200.0, 200.0);
    popover.begin().unwrap();
    route(&mut tree, &press(10.0, 50.0));
    route(&mut tree, &moved(70.0, 50.0));
    route(&mut tree, &release(70.0, 50.0));
    assert_eq!(value.peek(), 70.0);
    assert!(popover.is_open());
    popover.revert().unwrap();
    assert_eq!(value.peek(), 10.0);
}

fn told() -> Told {
    Rc::default()
}

#[test]
fn a_drag_let_go_starts_once_and_ends_kept_once() {
    let told = told();
    let mut canvas = canvas_reporting(20.0, Ends::told(&told));
    canvas.event(press(20.0, 50.0));
    canvas.event(moved(40.0, 50.0));
    canvas.event(moved(60.0, 50.0));
    assert_eq!(*told.borrow(), ["start"], "one start for the whole drag");
    canvas.event(release(60.0, 50.0));
    assert_eq!(*told.borrow(), ["start", "kept"]);
    assert_eq!(canvas.value.peek(), 60.0);

    canvas.event(press(60.0, 50.0));
    canvas.event(release(60.0, 50.0));
    assert_eq!(
        *told.borrow(),
        ["start", "kept", "start", "kept"],
        "and the next drag is a drag of its own"
    );
}

#[test]
fn escape_ends_the_drag_undone_and_the_release_after_it_says_nothing() {
    let told = told();
    let mut canvas = canvas_reporting(20.0, Ends::told(&told));
    canvas.event(press(20.0, 50.0));
    canvas.event(moved(80.0, 50.0));
    canvas.event(named(NamedKey::Escape));
    assert_eq!(*told.borrow(), ["start", "undone"]);
    assert_eq!(canvas.value.peek(), 20.0, "reverted by the time it ended");
    canvas.event(release(80.0, 50.0));
    assert_eq!(*told.borrow(), ["start", "undone"]);
}

#[test]
fn a_handle_taken_away_mid_drag_ends_it_undone() {
    fresh_layout_runtime();
    let told = told();
    let ends = Ends::told(&told);
    let value = signal(20.0);
    let mut props = HandleProps::props()
        .value(value)
        .to_point(Rc::new(|value| (value, 50.0)))
        .build();
    props.on_start = ends.on_start;
    props.on_end = ends.on_end;
    let mut dot = handle(props, Children::default()).unwrap();
    lay_out(dot.layout_node(), 200.0, 200.0);
    dot.on_event(&press(20.0, 50.0));
    dot.on_event(&moved(70.0, 50.0));
    assert_eq!(*told.borrow(), ["start"]);
    assert_eq!(value.peek(), 70.0);

    drop(dot);
    assert_eq!(*told.borrow(), ["start", "undone"]);
    assert_eq!(
        value.peek(),
        20.0,
        "its transaction reverted before it said so"
    );
}

#[test]
fn an_arrow_key_step_is_not_a_drag() {
    let told = told();
    let mut canvas = canvas_reporting(20.0, Ends::told(&told));
    canvas.event(press(20.0, 50.0));
    canvas.event(release(20.0, 50.0));
    told.borrow_mut().clear();
    canvas.event(named(NamedKey::ArrowRight));
    assert_eq!(canvas.value.peek(), 21.0);
    assert!(told.borrow().is_empty());
}
