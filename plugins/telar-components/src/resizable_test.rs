use std::cell::{Cell, RefCell};

use telar::testing::route;
use telar::testing::{hold, key_with, moved, named, press, release};
use telar::{ComponentList, Direction, DrawCommand, Event, Rect, focus, relayout_if_dirty};

use super::*;

const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 600.0;

fn fresh() {
    crate::test_support::fresh_layout_runtime();
    ui_core::reset_keyboard();
    focus::clear();
}

fn mount(props: ResizableProps) -> ComponentList {
    fresh();
    tree(resizable(props, Children::default()).unwrap())
}

fn tree(item: Box<dyn LayoutItem>) -> ComponentList {
    telar::testing::lay_out(item.layout_node(), WIDTH, HEIGHT);
    let tree = ComponentList::new(item);
    let _ = tree.commands();
    tree
}

fn send(tree: &mut ComponentList, event: Event) {
    route(tree, &event);
    relayout_if_dirty();
    let _ = tree.commands();
}

fn bound(size: RwSignal<Size>) -> ResizableProps {
    ResizableProps::props().size(size).build()
}

fn splitter(orientation: Orientation) -> focus::Exposed {
    focus::exposed()
        .into_iter()
        .find(|e| e.role == Role::Splitter && e.orientation == Some(orientation))
        .unwrap_or_else(|| panic!("a {orientation:?} splitter is exposed"))
}

fn width_handle() -> Rect {
    telar::track_layout(splitter(Orientation::Vertical).node)
        .unwrap()
        .get()
}

fn height_handle() -> Rect {
    telar::track_layout(splitter(Orientation::Horizontal).node)
        .unwrap()
        .get()
}

fn centre(rect: Rect) -> (f64, f64) {
    (
        (rect.x + rect.width / 2.0) as f64,
        (rect.y + rect.height / 2.0) as f64,
    )
}

fn corner() -> (f64, f64) {
    let end = width_handle();
    centre(Rect::new(end.x, end.y + end.height, end.width, end.width))
}

fn drag(tree: &mut ComponentList, from: (f64, f64), by: (f64, f64)) {
    let to = (from.0 + by.0, from.1 + by.1);
    for event in [
        press(from.0, from.1),
        moved(to.0, to.1),
        release(to.0, to.1),
    ] {
        send(tree, event);
    }
}

fn key(tree: &mut ComponentList, key: NamedKey) {
    send(tree, named(key));
}

#[test]
fn the_content_takes_the_bound_size_and_each_edge_reports_its_side() {
    let size = signal(Size::new(300.0, 200.0));
    let _tree = mount(
        ResizableProps::props()
            .size(size)
            .min(Size::new(100.0, 50.0))
            .max(Size::new(500.0, 400.0))
            .build(),
    );
    let width = splitter(Orientation::Vertical).value.unwrap();
    let height = splitter(Orientation::Horizontal).value.unwrap();
    assert_eq!((width.now, width.min, width.max), (300.0, 100.0, 500.0));
    assert_eq!((height.now, height.min, height.max), (200.0, 50.0, 400.0));
    assert_eq!(width_handle().x, 300.0);
    assert_eq!(height_handle().y, 200.0);
}

#[test]
fn an_unbound_resizable_starts_at_its_default_size() {
    let _tree = mount(ResizableProps::props().build());
    assert_eq!(splitter(Orientation::Vertical).value.unwrap().now, 240.0);
    assert_eq!(splitter(Orientation::Horizontal).value.unwrap().now, 160.0);
}

#[test]
fn each_edge_resizes_its_own_side_by_the_distance_dragged() {
    let size = signal(Size::new(300.0, 200.0));
    let mut tree = mount(bound(size));
    drag(&mut tree, centre(width_handle()), (50.0, 40.0));
    assert_eq!(size.get(), Size::new(350.0, 200.0));
    drag(&mut tree, centre(height_handle()), (25.0, -30.0));
    assert_eq!(size.get(), Size::new(350.0, 170.0));
}

#[test]
fn the_corner_resizes_both_sides_and_is_not_focusable() {
    let size = signal(Size::new(300.0, 200.0));
    let mut tree = mount(bound(size));
    drag(&mut tree, corner(), (40.0, 30.0));
    assert_eq!(size.get(), Size::new(340.0, 230.0));
    let focusable = focus::exposed()
        .iter()
        .filter(|e| e.role == Role::Splitter)
        .count();
    assert_eq!(focusable, 2, "the corner is a pointer's shortcut");
}

#[test]
fn a_centred_resizable_grows_by_twice_the_travel() {
    let size = signal(Size::new(300.0, 200.0));
    let mut tree = mount(
        ResizableProps::props()
            .size(size)
            .growth(ResizeGrowth::Centred)
            .build(),
    );
    drag(&mut tree, corner(), (20.0, 10.0));
    assert_eq!(size.get(), Size::new(340.0, 220.0));
}

#[test]
fn a_press_that_never_travels_leaves_the_size_alone() {
    let size = signal(Size::new(300.0, 200.0));
    let mut tree = mount(bound(size));
    let (x, y) = centre(width_handle());
    for event in [press(x, y), moved(x + 1.0, y), release(x + 1.0, y)] {
        send(&mut tree, event);
    }
    assert_eq!(size.get(), Size::new(300.0, 200.0));
}

#[test]
fn a_drag_keeps_to_the_limits_and_the_room() {
    let size = signal(Size::new(300.0, 200.0));
    let room = signal(Size::new(360.0, 1000.0));
    let mut tree = mount(
        ResizableProps::props()
            .size(size)
            .min(Size::new(100.0, 80.0))
            .max(Size::new(0.0, 260.0))
            .room(room)
            .build(),
    );
    drag(&mut tree, corner(), (500.0, 500.0));
    assert_eq!(size.get(), Size::new(360.0, 260.0));
    drag(&mut tree, corner(), (-900.0, -900.0));
    assert_eq!(size.get(), Size::new(100.0, 80.0));

    room.set(Size::new(50.0, 50.0));
    drag(&mut tree, corner(), (40.0, 40.0));
    assert_eq!(
        size.get(),
        Size::new(100.0, 80.0),
        "a room smaller than the minimum never shrinks the content past it"
    );
}

#[test]
fn escape_during_a_drag_restores_the_size() {
    let size = signal(Size::new(300.0, 200.0));
    let mut tree = mount(bound(size));
    let (x, y) = corner();
    send(&mut tree, press(x, y));
    send(&mut tree, moved(x + 40.0, y + 20.0));
    assert_eq!(size.get(), Size::new(340.0, 220.0));
    send(&mut tree, named(NamedKey::Escape));
    assert_eq!(size.get(), Size::new(300.0, 200.0));
}

#[test]
fn arrows_step_the_focused_edge_and_shift_steps_further() {
    let size = signal(Size::new(300.0, 200.0));
    let mut tree = mount(ResizableProps::props().size(size).step(10.0).build());
    focus::focus_next();
    key(&mut tree, NamedKey::ArrowRight);
    assert_eq!(size.get(), Size::new(310.0, 200.0));
    key(&mut tree, NamedKey::ArrowDown);
    assert_eq!(
        size.get(),
        Size::new(310.0, 200.0),
        "the width edge leaves the height alone"
    );

    focus::focus_next();
    key(&mut tree, NamedKey::ArrowUp);
    assert_eq!(size.get(), Size::new(310.0, 190.0));
    send(
        &mut tree,
        key_with(Key::Named(NamedKey::ArrowDown), hold(true, false)),
    );
    assert_eq!(
        size.get(),
        Size::new(310.0, 190.0 + 10.0 * telar::COARSE_STEP)
    );
}

#[test]
fn home_and_end_take_the_focused_edge_to_its_limits() {
    let size = signal(Size::new(300.0, 200.0));
    let mut tree = mount(
        ResizableProps::props()
            .size(size)
            .min(Size::new(120.0, 90.0))
            .max(Size::new(400.0, 0.0))
            .build(),
    );
    focus::focus_next();
    key(&mut tree, NamedKey::End);
    assert_eq!(size.get(), Size::new(400.0, 200.0));
    key(&mut tree, NamedKey::Home);
    assert_eq!(size.get(), Size::new(120.0, 200.0));

    focus::focus_next();
    key(&mut tree, NamedKey::End);
    assert_eq!(
        size.get(),
        Size::new(120.0, 200.0),
        "an unbounded side has no end to go to"
    );
    key(&mut tree, NamedKey::Home);
    assert_eq!(size.get(), Size::new(120.0, 90.0));
}

#[test]
fn right_to_left_puts_the_width_edge_on_the_left_and_mirrors_it() {
    telar::set_direction(Direction::Rtl);
    let size = signal(Size::new(300.0, 200.0));
    let mut tree = mount(ResizableProps::props().size(size).step(10.0).build());
    assert!(
        width_handle().x + width_handle().width <= height_handle().x,
        "the width edge sits at the inline end, left of the content the height edge spans"
    );
    focus::focus_next();
    key(&mut tree, NamedKey::ArrowLeft);
    assert_eq!(size.get(), Size::new(310.0, 200.0));
    drag(&mut tree, centre(width_handle()), (-30.0, 0.0));
    assert_eq!(size.get(), Size::new(340.0, 200.0));
    telar::set_direction(Direction::Ltr);
}

#[test]
fn one_edge_draws_one_splitter_and_no_corner() {
    let size = signal(Size::new(300.0, 200.0));
    let _tree = mount(
        ResizableProps::props()
            .size(size)
            .edges(ResizeEdges::Bottom)
            .build(),
    );
    let splitters: Vec<_> = focus::exposed()
        .into_iter()
        .filter(|e| e.role == Role::Splitter)
        .collect();
    assert_eq!(splitters.len(), 1);
    assert_eq!(splitters[0].orientation, Some(Orientation::Horizontal));
    assert_eq!(height_handle().width, 300.0);

    let _tree = mount(
        ResizableProps::props()
            .size(size)
            .edges(ResizeEdges::End)
            .build(),
    );
    assert_eq!(splitter(Orientation::Vertical).value.unwrap().now, 300.0);
    assert_eq!(width_handle().height, 200.0);
}

#[test]
fn the_edges_are_named_by_their_side_unless_told_otherwise() {
    telar::set_locale("en");
    let names = |props: ResizableProps| {
        let tree = mount(props);
        let mut names: Vec<String> = ui_core::accessibility::snapshot(&tree.commands())
            .into_iter()
            .filter(|node| node.role == Role::Splitter)
            .map(|node| node.name)
            .collect();
        names.sort();
        names
    };
    assert_eq!(
        names(ResizableProps::props().build()),
        ["Resize height", "Resize width"]
    );
    assert_eq!(
        names(
            ResizableProps::props()
                .width_label("Panel width")
                .height_label("Panel height")
                .build()
        ),
        ["Panel height", "Panel width"]
    );
}

#[test]
fn every_drag_starts_once_and_ends_once_with_whether_it_was_kept() {
    let heard: Rc<RefCell<Vec<String>>> = Rc::default();
    let (started, ended) = (heard.clone(), heard.clone());
    let mut tree = mount(
        ResizableProps::props()
            .on_start(Rc::new(move || started.borrow_mut().push("start".into())))
            .on_end(Rc::new(move |kept| {
                ended.borrow_mut().push(format!("end {kept}"))
            }))
            .build(),
    );
    let (x, y) = centre(width_handle());
    for event in [
        press(x, y),
        moved(x + 20.0, y),
        moved(x + 30.0, y),
        release(x + 30.0, y),
    ] {
        send(&mut tree, event);
    }
    let (x, y) = centre(width_handle());
    for event in [press(x, y), moved(x + 20.0, y), named(NamedKey::Escape)] {
        send(&mut tree, event);
    }
    assert_eq!(*heard.borrow(), ["start", "end true", "start", "end false"]);
}

fn lone_handle(props: ResizeHandleProps) -> ComponentList {
    fresh();
    let handle = resize_handle(props, Children::default()).unwrap();
    let row = Container::new(LayoutStyle::row().height(80.0), vec![handle]).unwrap();
    tree(box_item(row))
}

#[test]
fn a_lone_handle_asks_for_sizes_within_its_limits_at_its_scale() {
    let asked: Rc<Cell<Option<Size>>> = Rc::default();
    let sink = asked.clone();
    let mut tree = lone_handle(
        ResizeHandleProps::props()
            .size(Size::new(100.0, 80.0))
            .on_resize(Rc::new(move |size| sink.set(Some(size))))
            .min(Size::new(60.0, 60.0))
            .max(Size::new(120.0, 120.0))
            .scale(2.0)
            .build(),
    );
    drag(&mut tree, centre(width_handle()), (30.0, 0.0));
    assert_eq!(asked.get(), Some(Size::new(115.0, 80.0)));
    drag(&mut tree, centre(width_handle()), (300.0, 0.0));
    assert_eq!(asked.get(), Some(Size::new(120.0, 80.0)));
    drag(&mut tree, centre(width_handle()), (-300.0, 0.0));
    assert_eq!(asked.get(), Some(Size::new(60.0, 80.0)));
}

#[test]
fn a_cancelled_drag_on_a_lone_handle_asks_for_the_size_it_started_from() {
    let asked: Rc<RefCell<Vec<Size>>> = Rc::default();
    let sink = asked.clone();
    let mut tree = lone_handle(
        ResizeHandleProps::props()
            .size(Size::new(100.0, 80.0))
            .on_resize(Rc::new(move |size| sink.borrow_mut().push(size)))
            .build(),
    );
    let (x, y) = centre(width_handle());
    for event in [press(x, y), moved(x + 40.0, y), named(NamedKey::Escape)] {
        send(&mut tree, event);
    }
    assert_eq!(
        *asked.borrow(),
        [Size::new(140.0, 80.0), Size::new(100.0, 80.0)]
    );
}

#[derive(Clone, Default)]
struct Plain {
    resizables: Option<ResizableTokens>,
}

impl telar::ThemeTokens for Plain {
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(resizables) = &self.resizables {
            extensions.insert(resizables.clone());
        }
    }
}

fn fills(tree: &ComponentList) -> Vec<(Rect, Color)> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Rect { rect, style } => style.fill.map(|fill| (*rect, fill.solid_color())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_handle_is_a_spacing_step_thick_until_the_tokens_say_otherwise() {
    telar::set_theme(Plain::default());
    let _tree = mount(ResizableProps::props().build());
    assert_eq!(width_handle().width, crate::metrics::spacing());

    telar::set_theme(Plain {
        resizables: Some(ResizableTokens {
            thickness: Some(5.0),
            ..ResizableTokens::default()
        }),
    });
    let _tree = mount(ResizableProps::props().build());
    assert_eq!(width_handle().width, 5.0);
    assert_eq!(height_handle().height, 5.0);
}

#[test]
fn the_corner_shows_the_grip_and_a_handle_under_the_pointer_lights_up() {
    let (grip, active) = (Color::rgb(0.2, 0.6, 0.2), Color::rgb(0.8, 0.1, 0.1));
    telar::set_theme(Plain {
        resizables: Some(ResizableTokens {
            grip: Some(grip),
            active: Some(active),
            thickness: None,
        }),
    });
    let mut tree = mount(ResizableProps::props().build());
    let end = width_handle();
    assert_eq!(
        fills(&tree)
            .iter()
            .map(|(_, fill)| *fill)
            .collect::<Vec<_>>(),
        [grip],
        "only the corner is drawn at rest"
    );

    let (x, y) = centre(end);
    send(&mut tree, moved(x, y));
    assert!(
        fills(&tree).contains(&(end, active)),
        "the edge under the pointer takes the active colour"
    );
    telar::set_theme(Plain::default());
}

#[test]
fn a_style_amendment_reshapes_every_handle() {
    telar::set_theme(Plain::default());
    let tint = Color::rgb(0.9, 0.1, 0.1);
    let tree = mount(
        ResizableProps::props()
            .style(Rc::new(move |s: RectStyle| s.with_fill(tint)) as SurfaceAmendment)
            .build(),
    );
    let painted = fills(&tree);
    assert_eq!(painted.len(), 3);
    assert!(painted.iter().all(|(_, fill)| *fill == tint));
}
