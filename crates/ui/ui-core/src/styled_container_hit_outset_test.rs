use std::cell::{Cell, RefCell};
use std::rc::Rc;

use layout_core::AvailableSpace;
use platform_core::{PointerSource, ScrollDelta};

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::layout_item::box_item;

const BAR_START: f32 = 140.0;
const BAR: f32 = 6.0;
const REACH: f32 = 9.0;

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

fn to(x: f64, y: f64) -> Event {
    Event::PointerMoved {
        x,
        y,
        source: PointerSource::Mouse,
    }
}

fn wheel(x: f64, y: f64) -> Event {
    Event::Scrolled {
        delta: ScrollDelta::Pixels { x: 0.0, y: -10.0 },
        x,
        y,
    }
}

fn click(root: &mut Container, x: f64) {
    root.on_event(&press(x, 50.0));
    root.on_event(&release(x, 50.0));
}

#[derive(Default)]
struct Heard {
    presses: Cell<u32>,
    wheels: Cell<u32>,
    hovered: Cell<bool>,
}

impl Heard {
    fn new() -> Rc<Self> {
        Rc::new(Self::default())
    }
}

fn pane(layout: LayoutStyle, heard: &Rc<Heard>) -> StyledContainer {
    let (pressed, wheeled, hovered) = (heard.clone(), heard.clone(), heard.clone());
    StyledContainer::new(layout, |_| RectStyle::default(), vec![])
        .unwrap()
        .on_press(move || pressed.presses.set(pressed.presses.get() + 1))
        .on_scroll(move |_, _| wheeled.wheels.set(wheeled.wheels.get() + 1))
        .on_hover(move |inside| hovered.hovered.set(inside))
}

struct Split {
    root: Container,
    start: Rc<Heard>,
    bar: Rc<Heard>,
    end: Rc<Heard>,
    dragged: Rc<RefCell<Vec<(f32, f32)>>>,
}

/// A 300×100 row: a 140 px pane, a 6 px bar, and a pane taking the rest, the bar declared between them so the pane after it is stacked over it.
fn split(outset: bool) -> Split {
    reset_layout_runtime();
    let _ = platform_core::take_window_commands();
    let (start, bar_heard, end) = (Heard::new(), Heard::new(), Heard::new());
    let dragged = Rc::new(RefCell::new(Vec::new()));
    let first = pane(LayoutStyle::new().width(BAR_START).flex_shrink(0.0), &start);
    let second = pane(LayoutStyle::new().flex_grow(1.0), &end);
    let sink = dragged.clone();
    let bar = pane(LayoutStyle::new().width(BAR).flex_shrink(0.0), &bar_heard)
        .cursor(Cursor::ColResize)
        .on_drag(move |x, y| sink.borrow_mut().push((x, y)));
    let bar = if outset {
        bar.hit_outset(Insets::new(0.0, REACH, 0.0, REACH))
    } else {
        bar
    };
    let root = Container::new(
        LayoutStyle::new().flex_row().width(300.0).height(100.0),
        vec![box_item(first), box_item(bar), box_item(second)],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    Split {
        root,
        start,
        bar: bar_heard,
        end,
        dragged,
    }
}

const PAST_END: f64 = (BAR_START + BAR + REACH - 1.0) as f64;
const BEFORE_START: f64 = (BAR_START - REACH + 1.0) as f64;

#[test]
fn a_press_in_the_outset_is_the_box_s_over_the_sibling_stacked_on_it() {
    let mut split = split(true);
    click(&mut split.root, PAST_END);
    assert_eq!(split.bar.presses.get(), 1, "over the later pane");
    assert_eq!(split.end.presses.get(), 0);

    click(&mut split.root, BEFORE_START);
    assert_eq!(split.bar.presses.get(), 2, "over the earlier pane");
    assert_eq!(split.start.presses.get(), 0);

    click(&mut split.root, PAST_END + 2.0);
    assert_eq!(
        split.end.presses.get(),
        1,
        "past the outset the pane has it"
    );
    assert_eq!(split.bar.presses.get(), 2);
}

#[test]
fn without_an_outset_the_sibling_keeps_the_press() {
    let mut split = split(false);
    click(&mut split.root, PAST_END);
    assert_eq!(split.end.presses.get(), 1);
    assert_eq!(split.bar.presses.get(), 0);
}

#[test]
fn the_wheel_in_the_outset_scrolls_the_pane_beneath() {
    let mut split = split(true);
    split.root.on_event(&wheel(PAST_END, 50.0));
    assert_eq!(split.end.wheels.get(), 1, "the later pane");
    split.root.on_event(&wheel(BEFORE_START, 50.0));
    assert_eq!(split.start.wheels.get(), 1, "the earlier pane");
    assert_eq!(
        split.bar.wheels.get(),
        0,
        "the outset never takes the wheel"
    );
}

#[test]
fn the_outset_takes_the_hover_and_the_cursor_from_the_pane_it_lies_over() {
    let mut split = split(true);
    split.root.on_event(&to(PAST_END, 50.0));
    assert!(split.bar.hovered.get());
    assert!(
        !split.end.hovered.get(),
        "the pane under the outset is covered"
    );
    assert_eq!(crate::cursor::requested_cursor(), Cursor::ColResize);

    split.root.on_event(&to(PAST_END + 20.0, 50.0));
    assert!(!split.bar.hovered.get());
    assert!(split.end.hovered.get());
    assert_eq!(crate::cursor::requested_cursor(), Cursor::Default);
}

#[test]
fn a_drag_from_the_outset_reports_points_local_to_the_laid_out_box() {
    let mut split = split(true);
    split.root.on_event(&press(PAST_END, 50.0));
    split.root.on_event(&to(PAST_END + 10.0, 50.0));
    split.root.on_event(&release(PAST_END + 10.0, 50.0));
    let local = (PAST_END as f32) - BAR_START;
    assert_eq!(
        *split.dragged.borrow(),
        vec![(local, 50.0), (local + 10.0, 50.0)]
    );
}

#[test]
fn an_inert_box_gives_its_outset_back() {
    reset_layout_runtime();
    let (bar_heard, end) = (Heard::new(), Heard::new());
    let bar = pane(LayoutStyle::new().width(BAR).flex_shrink(0.0), &bar_heard)
        .hit_outset(Insets::new(0.0, REACH, 0.0, REACH))
        .inert(|| true);
    let second = pane(LayoutStyle::new().flex_grow(1.0), &end);
    let mut root = Container::new(
        LayoutStyle::new().flex_row().width(300.0).height(100.0),
        vec![box_item(bar), box_item(second)],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    click(&mut root, (BAR + 2.0) as f64);
    assert_eq!(end.presses.get(), 1);
    assert_eq!(bar_heard.presses.get(), 0);
}

#[test]
fn a_reader_and_the_input_region_are_told_the_grown_area() {
    reset_layout_runtime();
    focus::clear();
    let bar = StyledContainer::new(
        LayoutStyle::new().width(BAR).flex_shrink(0.0),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(focus::Role::Splitter)
    .on_drag(|_, _| {})
    .hit_outset(Insets::new(0.0, REACH, 0.0, REACH));
    let root = Container::new(
        LayoutStyle::new().flex_row().width(300.0).height(100.0),
        vec![
            box_item(Container::new(LayoutStyle::new().width(BAR_START), vec![]).unwrap()),
            box_item(bar),
        ],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(300.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    let grown = Rect::new(BAR_START - REACH, 0.0, BAR + 2.0 * REACH, 100.0);

    let nodes = crate::accessibility::snapshot(&[]);
    let splitter = nodes
        .iter()
        .find(|node| node.role == focus::Role::Splitter)
        .expect("the splitter is exposed");
    assert_eq!(splitter.rect, grown);
    assert_eq!(input_region::interactive_rects(), vec![grown]);
}
