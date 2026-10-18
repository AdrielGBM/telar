use telar::testing::{hold, moved, named, press, release};
use telar::{Component, LayoutItem, NamedKey, Orientation, Slots, Text, focus};

use super::*;

const WIDTH: f32 = 600.0;
const HEIGHT: f32 = 300.0;

fn pane_content() -> Slots {
    let mut slots = Slots::new();
    for label in ["first", "second"] {
        let text = Text::declaring(move || label.to_string(), LayoutStyle::new(), |t| t).unwrap();
        slots.push(None, box_item(text));
    }
    slots
}

struct Split {
    item: Box<dyn LayoutItem>,
    root: RwSignal<Rect>,
    direction: SplitDirection,
    sized: SizedPane,
    bar: f32,
}

impl Split {
    fn build(props: SplitPaneProps) -> Self {
        Self::holding(props, pane_content)
    }

    fn holding(props: SplitPaneProps, content: impl FnOnce() -> Slots) -> Self {
        crate::test_support::fresh_layout_runtime();
        ui_core::reset_keyboard();
        focus::clear();
        let (direction, sized) = (props.direction, props.sized);
        let item = split_pane(props, Children::from(content())).unwrap();
        let node = item.layout_node();
        let root = telar::track_layout(node).unwrap();
        telar::testing::lay_out(node, WIDTH, HEIGHT);
        Self {
            item,
            root,
            direction,
            sized,
            bar: bar_thickness(),
        }
    }

    fn applied(&self) -> f32 {
        focus::exposed()
            .iter()
            .find(|e| e.role == Role::Splitter)
            .and_then(|e| e.value)
            .map_or(f32::NAN, |value| value.now as f32)
    }

    fn bar_centre(&self) -> (f64, f64) {
        let root = self.root.get();
        let along = match self.sized {
            SizedPane::First => self.applied() + self.bar / 2.0,
            SizedPane::Second => self.direction.main(root) - self.applied() - self.bar / 2.0,
        };
        match self.direction {
            SplitDirection::Row => ((root.x + along) as f64, (root.y + root.height / 2.0) as f64),
            SplitDirection::Column => ((root.x + root.width / 2.0) as f64, (root.y + along) as f64),
        }
    }

    fn across(&self, offset: f64) -> (f64, f64) {
        let (x, y) = self.bar_centre();
        match self.direction {
            SplitDirection::Row => (x + offset, y),
            SplitDirection::Column => (x, y + offset),
        }
    }

    fn drag_by(&mut self, by: f64) {
        self.drag_from(0.0, by);
    }

    fn drag_from(&mut self, offset: f64, by: f64) {
        let (x, y) = self.across(offset);
        let (to_x, to_y) = self.across(offset + by);
        self.item.on_event(&press(x, y));
        self.item.on_event(&moved(to_x, to_y));
        self.item.on_event(&release(to_x, to_y));
    }

    fn key(&mut self, key: NamedKey) {
        self.item.on_event(&named(key));
    }
}

fn props(size: RwSignal<f32>) -> SplitPaneProps {
    SplitPaneProps::props().size(size).build()
}

#[test]
fn the_first_pane_takes_the_bound_size() {
    let size = signal(200.0f32);
    let split = Split::build(props(size));
    assert_eq!(split.applied(), 200.0);
}

#[test]
fn dragging_the_splitter_resizes_the_first_pane() {
    let size = signal(200.0f32);
    let mut split = Split::build(props(size));
    split.drag_by(60.0);
    assert_eq!(size.get(), 260.0);
}

#[test]
fn dragging_past_the_limits_clamps_to_them() {
    let size = signal(200.0f32);
    let mut split = Split::build(
        SplitPaneProps::props()
            .size(size)
            .min(150.0)
            .max(320.0)
            .build(),
    );
    split.drag_by(500.0);
    assert_eq!(size.get(), 320.0);
    split.drag_by(-900.0);
    assert_eq!(size.get(), 150.0);
}

#[test]
fn fractions_bound_the_size_by_the_whole_split() {
    let size = signal(200.0f32);
    let mut split = Split::build(
        SplitPaneProps::props()
            .size(size)
            .min_fraction(0.25)
            .max_fraction(0.5)
            .build(),
    );
    split.drag_by(800.0);
    assert_eq!(size.get(), 300.0);
    split.drag_by(-800.0);
    assert_eq!(size.get(), 150.0);
}

#[test]
fn escape_during_a_drag_restores_the_size() {
    let size = signal(200.0f32);
    let split = Split::build(props(size));
    let (x, y) = split.bar_centre();
    let mut tree = telar::ComponentList::new(split.item);
    telar::testing::route(&mut tree, &press(x, y));
    telar::testing::route(&mut tree, &moved(x + 40.0, y));
    assert_eq!(size.get(), 240.0);
    telar::testing::route(&mut tree, &named(NamedKey::Escape));
    assert_eq!(size.get(), 200.0);
}

#[test]
fn a_double_click_collapses_and_restores_the_first_pane() {
    let size = signal(200.0f32);
    let collapsed = signal(false);
    let mut split = Split::build(
        SplitPaneProps::props()
            .size(size)
            .collapsed(collapsed)
            .build(),
    );
    let (x, y) = split.bar_centre();
    for _ in 0..2 {
        split.item.on_event(&press(x, y));
        split.item.on_event(&release(x, y));
    }
    assert!(collapsed.get());
    assert_eq!(size.get(), 200.0, "the size to restore is kept");

    for _ in 0..2 {
        split.item.on_event(&press(x, y));
        split.item.on_event(&release(x, y));
    }
    assert!(!collapsed.get());
}

#[test]
fn a_collapsible_off_pane_ignores_double_clicks() {
    let size = signal(200.0f32);
    let collapsed = signal(false);
    let mut split = Split::build(
        SplitPaneProps::props()
            .size(size)
            .collapsed(collapsed)
            .collapsible(false)
            .build(),
    );
    let (x, y) = split.bar_centre();
    for _ in 0..2 {
        split.item.on_event(&press(x, y));
        split.item.on_event(&release(x, y));
    }
    assert!(!collapsed.get());
}

#[test]
fn arrows_step_the_size_and_shift_steps_further() {
    let size = signal(200.0f32);
    let mut split = Split::build(SplitPaneProps::props().size(size).step(10.0).build());
    focus::focus_next();

    hold(false, false);
    split.key(NamedKey::ArrowRight);
    assert_eq!(size.get(), 210.0);
    split.key(NamedKey::ArrowLeft);
    split.key(NamedKey::ArrowLeft);
    assert_eq!(size.get(), 190.0);

    hold(true, false);
    split.key(NamedKey::ArrowRight);
    assert_eq!(size.get(), 190.0 + 10.0 * telar::COARSE_STEP);
    hold(false, false);
}

#[test]
fn home_and_end_go_to_the_limits() {
    let size = signal(200.0f32);
    let mut split = Split::build(
        SplitPaneProps::props()
            .size(size)
            .min(120.0)
            .max(400.0)
            .build(),
    );
    focus::focus_next();
    split.key(NamedKey::End);
    assert_eq!(size.get(), 400.0);
    split.key(NamedKey::Home);
    assert_eq!(size.get(), 120.0);
}

#[test]
fn enter_toggles_the_collapse() {
    let collapsed = signal(false);
    let mut split = Split::build(SplitPaneProps::props().collapsed(collapsed).build());
    focus::focus_next();
    split.key(NamedKey::Enter);
    assert!(collapsed.get());
    split.key(NamedKey::Enter);
    assert!(!collapsed.get());
}

#[test]
fn an_arrow_on_a_collapsed_pane_reopens_it() {
    let size = signal(200.0f32);
    let collapsed = signal(true);
    let mut split = Split::build(
        SplitPaneProps::props()
            .size(size)
            .collapsed(collapsed)
            .build(),
    );
    focus::focus_next();
    split.key(NamedKey::ArrowRight);
    assert!(!collapsed.get());
    assert_eq!(size.get(), 200.0);
}

#[test]
fn the_bound_size_moves_the_pane_when_the_caller_sets_it() {
    let size = signal(200.0f32);
    let split = Split::build(props(size));
    size.set(320.0);
    telar::testing::lay_out(split.item.layout_node(), WIDTH, HEIGHT);
    assert_eq!(split.applied(), 320.0);
}

#[test]
fn the_collapsed_pane_has_no_extent() {
    let collapsed = signal(true);
    let split = Split::build(SplitPaneProps::props().collapsed(collapsed).build());
    assert_eq!(split.applied(), 0.0);
}

#[test]
fn an_unbound_split_owns_its_size() {
    let mut split = Split::build(SplitPaneProps::props().default_size(180.0).build());
    assert_eq!(split.applied(), 180.0);
    split.drag_by(20.0);
}

#[test]
fn a_stacked_split_drags_along_the_vertical_axis() {
    let size = signal(100.0f32);
    let mut split = Split::build(
        SplitPaneProps::props()
            .direction(SplitDirection::Column)
            .size(size)
            .build(),
    );
    let (x, y) = split.bar_centre();
    split.item.on_event(&press(x, y));
    split.item.on_event(&moved(x, y + 30.0));
    split.item.on_event(&release(x, y + 30.0));
    assert_eq!(size.get(), 130.0);
}

#[test]
fn the_splitter_reports_its_value_and_orientation() {
    let size = signal(200.0f32);
    let split = Split::build(
        SplitPaneProps::props()
            .size(size)
            .min(100.0)
            .max(400.0)
            .build(),
    );
    let exposed = focus::exposed();
    let splitter = exposed
        .iter()
        .find(|e| e.role == Role::Splitter)
        .expect("the splitter is exposed");
    let value = splitter.value.expect("it carries a value");
    assert_eq!((value.now, value.min, value.max), (200.0, 100.0, 400.0));
    assert_eq!(splitter.orientation, Some(Orientation::Vertical));
    drop(split);
}

#[test]
fn a_stacked_splitter_is_a_horizontal_bar() {
    let _split = Split::build(
        SplitPaneProps::props()
            .direction(SplitDirection::Column)
            .build(),
    );
    let exposed = focus::exposed();
    let splitter = exposed.iter().find(|e| e.role == Role::Splitter).unwrap();
    assert_eq!(splitter.orientation, Some(Orientation::Horizontal));
}

/// What a reader calls the splitter of a split built from `props`.
fn splitter_name(props: SplitPaneProps) -> String {
    let split = Split::build(props);
    let tree = telar::ComponentList::new(split.item);
    let nodes = ui_core::accessibility::snapshot(&tree.commands());
    nodes
        .iter()
        .find(|node| node.role == Role::Splitter)
        .map(|node| node.name.clone())
        .expect("the splitter is announced")
}

#[test]
fn the_splitter_is_named_resize_unless_told_otherwise() {
    telar::set_locale("en");
    assert_eq!(splitter_name(SplitPaneProps::props().build()), "Resize");
    assert_eq!(
        splitter_name(SplitPaneProps::props().label("Sidebar width").build()),
        "Sidebar width"
    );
}

fn second(size: RwSignal<f32>, direction: SplitDirection) -> SplitPaneProps {
    SplitPaneProps::props()
        .direction(direction)
        .sized(SizedPane::Second)
        .size(size)
        .build()
}

#[test]
fn a_sized_second_pane_takes_the_bound_size() {
    let size = signal(200.0f32);
    let split = Split::build(second(size, SplitDirection::Row));
    assert_eq!(split.applied(), 200.0);
    size.set(260.0);
    telar::testing::lay_out(split.item.layout_node(), WIDTH, HEIGHT);
    assert_eq!(split.applied(), 260.0);
}

#[test]
fn dragging_toward_a_sized_second_pane_shrinks_it() {
    let size = signal(200.0f32);
    let mut split = Split::build(second(size, SplitDirection::Row));
    split.drag_by(60.0);
    assert_eq!(size.get(), 140.0);
    telar::testing::lay_out(split.item.layout_node(), WIDTH, HEIGHT);
    split.drag_by(-100.0);
    assert_eq!(size.get(), 240.0);
}

#[test]
fn a_sized_bottom_pane_grows_as_the_splitter_rises() {
    let size = signal(100.0f32);
    let mut split = Split::build(second(size, SplitDirection::Column));
    focus::focus_next();
    hold(false, false);
    split.key(NamedKey::ArrowUp);
    assert_eq!(size.get(), 116.0);
    split.key(NamedKey::ArrowDown);
    split.key(NamedKey::ArrowDown);
    assert_eq!(size.get(), 84.0);
}

#[test]
fn fractions_bound_a_sized_second_pane_by_the_whole_split() {
    let size = signal(200.0f32);
    let mut split = Split::build(
        SplitPaneProps::props()
            .sized(SizedPane::Second)
            .size(size)
            .min(160.0)
            .max_fraction(0.6)
            .build(),
    );
    split.drag_by(-900.0);
    assert_eq!(size.get(), WIDTH * 0.6);
    split.drag_by(900.0);
    assert_eq!(size.get(), 160.0);
}

#[test]
fn a_collapsed_second_pane_has_no_extent() {
    let collapsed = signal(true);
    let split = Split::build(
        SplitPaneProps::props()
            .sized(SizedPane::Second)
            .collapsed(collapsed)
            .build(),
    );
    assert_eq!(split.applied(), 0.0);
}

#[test]
fn a_collapsed_pane_takes_its_controls_out_of_the_tab_order() {
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let collapsed = signal(true);
    let mut slots = Slots::new();
    for _ in 0..2 {
        let control = StyledContainer::new(LayoutStyle::new(), |_| RectStyle::default(), vec![])
            .unwrap()
            .control(Role::Button);
        slots.push(None, box_item(control));
    }
    let item = split_pane(
        SplitPaneProps::props().collapsed(collapsed).build(),
        Children::from(slots),
    )
    .unwrap();
    telar::testing::lay_out(item.layout_node(), WIDTH, HEIGHT);
    focus::focus_next();
    let first = focus::exposed()
        .into_iter()
        .find(|e| Some(e.id) == focus::current())
        .map(|e| e.role);
    assert_eq!(first, Some(Role::Splitter));
}

fn splitter_rect() -> Rect {
    let node = focus::exposed()
        .iter()
        .find(|e| e.role == Role::Splitter)
        .expect("the splitter is exposed")
        .node;
    telar::track_layout(node).unwrap().get()
}

/// Just inside the far edge of the grab strip, measured from the middle of the gap: over a pane, not over the gap.
fn grab_edge() -> f64 {
    (bar_thickness() * 0.75) as f64
}

fn sized(direction: SplitDirection, size: RwSignal<f32>) -> SplitPaneProps {
    SplitPaneProps::props()
        .direction(direction)
        .size(size)
        .collapsible(false)
        .build()
}

#[derive(Default)]
struct Heard {
    presses: Cell<u32>,
    wheels: Cell<u32>,
    laid: Cell<Option<RwSignal<Rect>>>,
}

fn heard() -> [Rc<Heard>; 2] {
    [Rc::new(Heard::default()), Rc::new(Heard::default())]
}

/// Each pane filled by a box that counts the presses and the wheel turns it hears.
fn listening(heard: &[Rc<Heard>; 2]) -> Slots {
    let mut slots = Slots::new();
    for pane in heard {
        let (pressed, wheeled) = (pane.clone(), pane.clone());
        let fill = StyledContainer::new(
            LayoutStyle::new().flex_grow(1.0),
            |_| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .on_press(move || pressed.presses.set(pressed.presses.get() + 1))
        .on_scroll(move |_, _| wheeled.wheels.set(wheeled.wheels.get() + 1));
        pane.laid.set(telar::track_layout(fill.layout_node()));
        slots.push(None, box_item(fill));
    }
    slots
}

#[test]
fn the_thin_bar_alone_divides_the_panes() {
    for direction in [SplitDirection::Row, SplitDirection::Column] {
        divided_by_the_thin_bar_alone(direction);
    }
}

fn divided_by_the_thin_bar_alone(direction: SplitDirection) {
    let panes = heard();
    let split = Split::holding(sized(direction, signal(200.0)), || listening(&panes));
    let bar = splitter_rect();
    assert_eq!(direction.main(bar), bar_thickness());
    assert_eq!(direction.along(bar.x, bar.y), 200.0);
    let second = panes[1]
        .laid
        .get()
        .expect("the second pane is laid out")
        .get();
    assert_eq!(
        direction.along(second.x, second.y),
        200.0 + bar_thickness(),
        "the second pane starts where the bar ends"
    );
    let range = focus::exposed()
        .iter()
        .find(|e| e.role == Role::Splitter)
        .and_then(|e| e.value)
        .map(|value| value.max as f32);
    assert_eq!(
        range,
        Some(direction.main(split.root.get()) - bar_thickness()),
        "the pane may take all but the bar"
    );
}

#[test]
fn the_grab_strip_reaches_half_a_gap_over_each_pane_and_no_further() {
    for direction in [SplitDirection::Row, SplitDirection::Column] {
        reaches_half_a_gap_over_each_pane(direction);
    }
}

fn reaches_half_a_gap_over_each_pane(direction: SplitDirection) {
    let panes = heard();
    let split = Split::holding(sized(direction, signal(200.0)), || listening(&panes));
    let past: Vec<_> = [-1.0, 1.0]
        .into_iter()
        .map(|side| split.across(side * f64::from(bar_thickness() + 1.0)))
        .collect();
    let mut tree = telar::ComponentList::new(split.item);
    let target = ui_core::accessibility::snapshot(&tree.commands())
        .into_iter()
        .find(|node| node.role == Role::Splitter)
        .expect("the splitter is announced")
        .rect;
    assert_eq!(direction.main(target), 2.0 * bar_thickness());
    for (pane, (x, y)) in panes.iter().zip(past) {
        telar::testing::route(&mut tree, &press(x, y));
        telar::testing::route(&mut tree, &release(x, y));
        assert_eq!(
            pane.presses.get(),
            1,
            "past the strip, what the pane keeps at its edge takes the press"
        );
    }
}

#[test]
fn a_drag_lights_a_rule_in_the_middle_of_the_gap() {
    for divider in [true, false] {
        let split = Split::build(
            SplitPaneProps::props()
                .size(signal(200.0))
                .divider(divider)
                .build(),
        );
        let (x, y) = split.bar_centre();
        let mut tree = telar::ComponentList::new(split.item);
        let at_rest = divider.then(|| shared::border().into());
        assert_eq!(rule_fill(&tree), at_rest, "at rest");
        telar::testing::route(&mut tree, &press(x, y));
        telar::testing::route(&mut tree, &moved(x + 20.0, y));
        assert_eq!(
            rule_fill(&tree),
            Some(shared::muted().into()),
            "while dragged"
        );
        telar::testing::route(&mut tree, &release(x + 20.0, y));
        assert_eq!(rule_fill(&tree), at_rest, "let go");
    }
}

fn rule_fill(tree: &telar::ComponentList) -> Option<telar::Paint> {
    tree.commands().iter().find_map(|command| match command {
        telar::DrawCommand::Rect { rect, style } if rect.width == rule_width() => style.fill,
        _ => None,
    })
}

#[test]
fn dragging_from_either_edge_of_the_grab_area_resizes() {
    for direction in [SplitDirection::Row, SplitDirection::Column] {
        drags_from_either_edge_of_the_grab_area(direction);
    }
}

fn drags_from_either_edge_of_the_grab_area(direction: SplitDirection) {
    let size = signal(200.0f32);
    let mut split = Split::build(sized(direction, size));
    split.drag_from(-grab_edge(), 40.0);
    assert_eq!(size.get(), 240.0, "from the edge over the first pane");
    telar::testing::lay_out(split.item.layout_node(), WIDTH, HEIGHT);
    split.drag_from(grab_edge(), 40.0);
    assert_eq!(size.get(), 280.0, "from the edge over the second pane");
}

#[test]
fn the_grab_area_takes_the_press_but_not_the_wheel_from_the_pane_beneath() {
    for direction in [SplitDirection::Row, SplitDirection::Column] {
        takes_the_press_but_not_the_wheel(direction);
    }
}

fn takes_the_press_but_not_the_wheel(direction: SplitDirection) {
    let size = signal(200.0f32);
    let panes = heard();
    let mut split = Split::holding(sized(direction, size), || listening(&panes));
    for (pane, side) in panes.iter().zip([-1.0, 1.0]) {
        let (x, y) = split.across(side * grab_edge());
        split.item.on_event(&telar::Event::Scrolled {
            delta: telar::ScrollDelta::Pixels { x: 0.0, y: -40.0 },
            x,
            y,
        });
        assert_eq!(
            pane.wheels.get(),
            1,
            "{side}: the pane under the grab area scrolls"
        );
        split.item.on_event(&press(x, y));
        split.item.on_event(&release(x, y));
        assert_eq!(pane.presses.get(), 0, "{side}: the splitter took the press");
    }
    assert!(focus::current().is_some(), "the press focused the splitter");
    assert_eq!(size.get(), 200.0);
}

const SPLITS: [(SplitDirection, SizedPane); 4] = [
    (SplitDirection::Row, SizedPane::First),
    (SplitDirection::Row, SizedPane::Second),
    (SplitDirection::Column, SizedPane::First),
    (SplitDirection::Column, SizedPane::Second),
];

fn in_region(
    direction: SplitDirection,
    sized: SizedPane,
    size: RwSignal<f32>,
    collapsed: RwSignal<bool>,
) -> SplitPaneProps {
    SplitPaneProps::props()
        .direction(direction)
        .sized(sized)
        .size(size)
        .collapsed(collapsed)
        .region(SplitRegion::new(Role::Main, "Workspace"))
        .build()
}

/// Each text drawn and the splitter, in draw order, with whether it is drawn inside the main landmark.
fn inside_main(commands: &[telar::DrawCommand]) -> Vec<(String, bool)> {
    let mut open = Vec::new();
    let mut drawn = Vec::new();
    for command in commands {
        match command {
            telar::DrawCommand::PushElement { element } => {
                if element.semantics.role == Role::Splitter {
                    drawn.push(("splitter".to_string(), open.contains(&Role::Main)));
                }
                open.push(element.semantics.role);
            }
            telar::DrawCommand::PopElement => {
                open.pop();
            }
            telar::DrawCommand::Text { text, .. } => {
                drawn.push((text.to_string(), open.contains(&Role::Main)));
            }
            _ => {}
        }
    }
    drawn
}

#[test]
fn a_region_holds_the_pane_that_takes_the_rest_and_the_splitter() {
    for (direction, sized) in SPLITS {
        let split = Split::build(in_region(direction, sized, signal(200.0), signal(false)));
        let tree = telar::ComponentList::new(split.item);
        let rest_first = sized == SizedPane::Second;
        let expected = [
            ("first".to_string(), rest_first),
            ("splitter".to_string(), true),
            ("second".to_string(), !rest_first),
        ];
        assert_eq!(
            inside_main(&tree.commands()),
            expected,
            "{direction:?} {sized:?}"
        );
        let named = ui_core::accessibility::snapshot(&tree.commands())
            .into_iter()
            .any(|node| node.role == Role::Main && node.name == "Workspace");
        assert!(
            named,
            "{direction:?} {sized:?}: the region is announced by its name"
        );
    }
}

#[test]
fn a_region_leaves_the_panes_where_they_were() {
    for (direction, sized) in SPLITS {
        let split = Split::build(in_region(direction, sized, signal(200.0), signal(false)));
        assert_eq!(split.applied(), 200.0, "{direction:?} {sized:?}");
        let bar = splitter_rect();
        let start = match sized {
            SizedPane::First => 200.0,
            SizedPane::Second => direction.main(split.root.get()) - 200.0 - bar_thickness(),
        };
        assert_eq!(
            direction.along(bar.x, bar.y),
            start,
            "{direction:?} {sized:?}"
        );
        assert_eq!(direction.main(bar), bar_thickness());
    }
}

#[test]
fn the_splitter_stays_in_the_region_and_reopens_a_collapsed_pane() {
    for (direction, sized) in SPLITS {
        let collapsed = signal(true);
        let split = Split::build(in_region(direction, sized, signal(200.0), collapsed));
        assert_eq!(split.applied(), 0.0, "{direction:?} {sized:?}");
        let mut tree = telar::ComponentList::new(split.item);
        let drawn = inside_main(&tree.commands());
        assert!(
            drawn.contains(&("splitter".to_string(), true)),
            "{direction:?} {sized:?}: {drawn:?}"
        );
        focus::focus_next();
        telar::testing::route(&mut tree, &named(NamedKey::Enter));
        assert!(!collapsed.get(), "{direction:?} {sized:?}");
    }
}

/// The middle of the splitter's bar, and the point `offset` px from it across the bar, toward the end of the split.
fn across_bar(direction: SplitDirection, offset: f64) -> (f64, f64) {
    let bar = splitter_rect();
    let (x, y) = (
        (bar.x + bar.width / 2.0) as f64,
        (bar.y + bar.height / 2.0) as f64,
    );
    match direction {
        SplitDirection::Row => (x + offset, y),
        SplitDirection::Column => (x, y + offset),
    }
}

/// Every split, in a region, dragged from just inside each edge of the grab area and wheeled there: the sized pane, outside the region, never takes the press from the splitter and still takes the wheel.
fn grabbed_from_either_edge_in_a_region(direction: SplitDirection, sized: SizedPane) {
    let size = signal(200.0f32);
    let panes = heard();
    let mut split = Split::holding(in_region(direction, sized, size, signal(false)), || {
        listening(&panes)
    });
    let before = match direction.inverted() {
        true => &panes[1],
        false => &panes[0],
    };
    let after = match direction.inverted() {
        true => &panes[0],
        false => &panes[1],
    };
    let reading = if direction.inverted() { -1.0 } else { 1.0 };
    let grows = sized.toward_end() * reading * 40.0;
    for (side, pane) in [(-1.0, before), (1.0, after)] {
        let from = size.get();
        let (x, y) = across_bar(direction, side * grab_edge());
        split.item.on_event(&telar::Event::Scrolled {
            delta: telar::ScrollDelta::Pixels { x: 0.0, y: -40.0 },
            x,
            y,
        });
        assert_eq!(
            pane.wheels.get(),
            1,
            "{direction:?} {sized:?} {side}: the pane under the grab area scrolls"
        );
        let (to_x, to_y) = across_bar(direction, side * grab_edge() + 40.0);
        split.item.on_event(&press(x, y));
        split.item.on_event(&moved(to_x, to_y));
        split.item.on_event(&release(to_x, to_y));
        assert_eq!(
            pane.presses.get(),
            0,
            "{direction:?} {sized:?} {side}: the splitter took the press"
        );
        assert_eq!(size.get(), from + grows, "{direction:?} {sized:?} {side}");
        telar::testing::lay_out(split.item.layout_node(), WIDTH, HEIGHT);
    }
}

#[test]
fn a_splitter_in_a_region_is_grabbed_from_either_edge() {
    for (direction, sized) in SPLITS {
        grabbed_from_either_edge_in_a_region(direction, sized);
    }
}

#[test]
fn a_splitter_in_a_region_is_grabbed_from_either_edge_right_to_left() {
    telar::set_direction(Direction::Rtl);
    for sized in [SizedPane::First, SizedPane::Second] {
        grabbed_from_either_edge_in_a_region(SplitDirection::Row, sized);
    }
    telar::set_direction(Direction::Ltr);
}

#[derive(Clone, Default)]
struct Plain {
    splits: Option<SplitPaneTokens>,
}

impl telar::ThemeTokens for Plain {
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(splits) = &self.splits {
            extensions.insert(splits.clone());
        }
    }
}

fn bar_paint(style: Option<SurfaceAmendment>) -> (Rect, RectStyle) {
    let mut props = SplitPaneProps::props().build();
    props.style = style;
    let split = Split::build(props);
    telar::ComponentList::new(split.item)
        .commands()
        .iter()
        .find_map(|command| match command {
            telar::DrawCommand::Rect { rect, style } if style.fill.is_some() => {
                Some((*rect, **style))
            }
            _ => None,
        })
        .expect("the rule paints")
}

#[test]
fn the_rule_is_a_pill_half_a_step_wide_until_the_tokens_say_otherwise() {
    telar::set_theme(Plain::default());
    let (bar, style) = bar_paint(None);
    assert_eq!(bar.width, crate::metrics::gap_sm() / 2.0);
    assert_eq!(style.radius, BorderRadius::all(bar.width / 2.0));

    telar::set_theme(Plain {
        splits: Some(SplitPaneTokens {
            thickness: Some(1.0),
            radius: Some(0.0),
        }),
    });
    let (bar, style) = bar_paint(None);
    assert_eq!(bar.width, 1.0);
    assert_eq!(style.radius, BorderRadius::all(0.0));
}

#[test]
fn a_hairline_gap_is_still_grabbed_beside_it() {
    telar::set_theme(Plain {
        splits: Some(SplitPaneTokens {
            thickness: Some(1.0),
            radius: None,
        }),
    });
    let size = signal(200.0f32);
    let mut split = Split::build(props(size));
    assert_eq!(split.bar, 1.0);
    split.drag_from(grab_edge(), 30.0);
    assert_eq!(size.get(), 230.0);
}

#[test]
fn a_style_amendment_reshapes_the_bar() {
    telar::set_theme(Plain::default());
    let tint: SurfaceAmendment =
        Rc::new(|s: RectStyle| s.with_fill(telar::Color::rgb(0.9, 0.1, 0.1)));
    let (_, style) = bar_paint(Some(tint));
    assert_eq!(
        style.fill.map(|f| f.solid_color()),
        Some(telar::Color::rgb(0.9, 0.1, 0.1))
    );
}
