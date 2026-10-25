use std::cell::Cell;
use std::rc::Rc;

use telar::testing::{hold, key_with, lay_out, moved, named, press, rect_of, release, route};
use telar::{ComponentList, Key, NamedKey, Rect, RwSignal, TextStyle, Transaction, signal};

use super::*;
use crate::test_support::fresh_layout_runtime;

struct Field {
    tree: ComponentList,
    value: RwSignal<f32>,
    commits: Rc<Cell<u32>>,
    y: f64,
}

fn field_over(value: RwSignal<f32>, transaction: Transaction<f32>, min: f32, max: f32) -> Field {
    let commits = Rc::new(Cell::new(0));
    let counted = commits.clone();
    let transaction = transaction.on_commit(move |_, _| counted.set(counted.get() + 1));
    let widget = scrub_field(
        ScrubFieldProps::props()
            .transaction(transaction)
            .label("W")
            .min(min)
            .max(max)
            .build(),
        Children::default(),
    )
    .unwrap();
    let node = widget.layout_node();
    let tree = ComponentList::new(widget);
    let rect = lay_out(node, 300.0, 40.0);
    Field {
        tree,
        value,
        commits,
        y: (rect.y + rect.height / 2.0) as f64,
    }
}

fn fresh() {
    fresh_layout_runtime();
    telar::testing::reset_keyboard();
    telar::focus::clear();
}

fn field(start: f32) -> Field {
    fresh();
    let value = signal(start);
    field_over(value, Transaction::new(value), -1000.0, 1000.0)
}

impl Field {
    fn event(&mut self, event: telar::Event) {
        route(&mut self.tree, &event);
    }

    /// Presses the label, clears the scrub threshold, then travels `dx` more before releasing.
    fn scrub(&mut self, dx: f64) {
        let y = self.y;
        self.event(press(5.0, y));
        self.event(moved(10.0, y));
        self.event(moved(10.0 + dx, y));
        self.event(release(10.0 + dx, y));
    }

    fn click(&mut self) {
        let y = self.y;
        self.event(press(5.0, y));
        self.event(release(5.0, y));
    }

    fn key(&mut self, key: NamedKey, is_shift: bool, is_alt: bool) {
        let modifiers = hold(is_shift, is_alt);
        self.event(key_with(Key::Named(key), modifiers));
    }

    fn type_char(&mut self, c: char) {
        self.event(key_with(Key::Char(c), Default::default()));
    }
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn scrubbing_the_label_steps_once_per_four_pixels() {
    let mut field = field(0.0);
    field.scrub(40.0);
    assert!(close(field.value.peek(), 10.0), "{}", field.value.peek());
    assert_eq!(field.commits.get(), 1, "one drag, one edit");
}

#[test]
fn shift_scrubs_ten_times_coarser_and_alt_ten_times_finer() {
    let mut field = field(0.0);
    hold(true, false);
    field.scrub(40.0);
    assert!(
        close(field.value.peek(), 100.0),
        "shift: {}",
        field.value.peek()
    );

    field.value.set(0.0);
    hold(false, true);
    field.scrub(40.0);
    assert!(
        close(field.value.peek(), 1.0),
        "alt: {}",
        field.value.peek()
    );
}

#[test]
fn changing_gear_mid_scrub_does_not_jump_the_value() {
    let mut field = field(0.0);
    let y = field.y;
    field.event(press(5.0, y));
    field.event(moved(10.0, y));
    field.event(moved(30.0, y));
    assert!(close(field.value.peek(), 5.0));
    hold(true, false);
    field.event(moved(30.0, y));
    assert!(
        close(field.value.peek(), 5.0),
        "holding shift alone moved it"
    );
    field.event(moved(34.0, y));
    assert!(close(field.value.peek(), 15.0), "{}", field.value.peek());
    field.event(release(34.0, y));
}

#[test]
fn the_arrow_keys_step_under_the_same_modifiers() {
    let mut field = field(0.0);
    field.click();
    field.key(NamedKey::ArrowRight, false, false);
    assert!(close(field.value.peek(), 1.0));
    field.key(NamedKey::ArrowUp, true, false);
    assert!(close(field.value.peek(), 11.0));
    field.key(NamedKey::ArrowLeft, false, true);
    assert!(close(field.value.peek(), 10.9), "{}", field.value.peek());
    assert_eq!(field.commits.get(), 3, "each press is its own edit");
}

#[test]
fn the_arrow_keys_do_nothing_without_focus() {
    let mut field = field(0.0);
    field.key(NamedKey::ArrowRight, false, false);
    assert_eq!(field.value.peek(), 0.0);
}

#[test]
fn a_scrub_stops_at_the_bounds() {
    fresh();
    let value = signal(0.0);
    let mut field = field_over(value, Transaction::new(value), 0.0, 5.0);
    field.scrub(400.0);
    assert_eq!(value.peek(), 5.0);
    field.scrub(-400.0);
    assert_eq!(value.peek(), 0.0);
}

#[test]
fn a_double_click_opens_typed_entry_and_enter_commits_it() {
    let mut field = field(3.0);
    field.click();
    field.click();
    field.event(named(NamedKey::Backspace));
    field.type_char('4');
    field.type_char('2');
    field.event(named(NamedKey::Enter));
    assert!(close(field.value.peek(), 42.0), "{}", field.value.peek());
    assert_eq!(field.commits.get(), 1);
}

#[test]
fn typed_entry_that_does_not_parse_or_is_abandoned_changes_nothing() {
    let mut field = field(3.0);
    field.click();
    field.click();
    field.type_char('x');
    field.event(named(NamedKey::Enter));
    assert_eq!(field.value.peek(), 3.0, "`3x` is not a number");

    field.click();
    field.click();
    field.type_char('9');
    field.event(named(NamedKey::Escape));
    assert_eq!(field.value.peek(), 3.0, "escape abandons the draft");
    assert_eq!(field.commits.get(), 0);
}

#[test]
fn enter_on_the_focused_field_opens_typed_entry_too() {
    let mut field = field(1.0);
    field.click();
    field.key(NamedKey::Enter, false, false);
    field.event(named(NamedKey::Backspace));
    field.type_char('7');
    field.event(named(NamedKey::Enter));
    assert!(close(field.value.peek(), 7.0), "{}", field.value.peek());
}

#[test]
fn escape_mid_scrub_reverts_the_whole_drag() {
    let mut field = field(5.0);
    let y = field.y;
    field.event(press(5.0, y));
    field.event(moved(10.0, y));
    field.event(moved(50.0, y));
    assert!(close(field.value.peek(), 15.0));

    field.event(named(NamedKey::Escape));
    assert_eq!(field.value.peek(), 5.0, "escape put the value back");
    field.event(moved(90.0, y));
    field.event(release(90.0, y));
    assert_eq!(
        field.value.peek(),
        5.0,
        "and the rest of the stroke changed nothing"
    );
    assert_eq!(field.commits.get(), 0);
}

#[test]
fn a_scrub_inside_an_open_transaction_joins_it() {
    fresh();
    let value = signal(2.0);
    let popover = Transaction::new(value);
    let mut field = field_over(value, popover, -100.0, 100.0);
    popover.begin().unwrap();

    field.scrub(40.0);
    assert!(close(value.peek(), 12.0));
    assert!(
        popover.is_open(),
        "the scrub left the decision to the popover"
    );
    assert_eq!(field.commits.get(), 0);

    popover.revert().unwrap();
    assert_eq!(value.peek(), 2.0);
}

fn mounted(props: ScrubFieldProps) -> ComponentList {
    fresh();
    let widget = scrub_field(props, Children::default()).unwrap();
    let node = widget.layout_node();
    let tree = ComponentList::new(widget);
    lay_out(node, 300.0, 40.0);
    tree
}

fn handle(tree: &ComponentList) -> RectStyle {
    crate::test_support::painted_at(tree, crate::test_support::drawn(Role::SpinButton)[0])
}

#[test]
fn the_corner_is_the_theme_s_control_role_until_the_scrub_field_tokens_name_one() {
    crate::test_support::assert_corner_follows_the_role_until_the_tokens_name_one(
        || mounted(ScrubFieldProps::props().label("W").build()),
        |tree| handle(tree).radius,
        |_, role| BorderRadius::all(role),
        |radius| ScrubFieldTokens {
            radius: Some(radius),
            ..ScrubFieldTokens::default()
        },
    );
}

#[test]
fn a_style_amendment_reshapes_the_handle() {
    let flat: SurfaceAmendment = Rc::new(|s: RectStyle| s.with_radius(BorderRadius::all(0.0)));
    let tree = mounted(ScrubFieldProps::props().label("W").style(flat).build());
    let painted = handle(&tree);
    assert_eq!(painted.radius, BorderRadius::all(0.0));
    assert!(painted.fill.is_some(), "the rest of the paint is kept");
}

/// With no label there is no caption to space the value from, so the handle is the value and its padding, with no gap opened before it.
#[test]
fn an_unlabelled_handle_holds_only_its_value_and_its_padding() {
    fresh();
    let widget = scrub_field(
        ScrubFieldProps::props().value(signal(7.0)).build(),
        Children::default(),
    )
    .unwrap();
    let node = widget.layout_node();
    let tree = ComponentList::new(widget);
    lay_out(node, 300.0, 40.0);
    let commands = tree.commands();
    let handle = commands
        .iter()
        .find_map(|command| match command {
            telar::DrawCommand::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("the handle is painted");
    let value = commands
        .iter()
        .find_map(|command| match command {
            telar::DrawCommand::Text { text, rect, .. } if text.as_ref() == "7" => Some(*rect),
            _ => None,
        })
        .expect("the value is drawn");
    assert_eq!(
        handle.width,
        value.width + 2.0 * ScrubFieldTokens::current().pad_x()
    );
}

fn drawn_alone_in(height: f32, text: &'static str, style: fn(TextStyle) -> TextStyle) -> Rect {
    let lone = Text::declaring(move || text.to_string(), LayoutStyle::new(), style).unwrap();
    let row = Container::new(LayoutStyle::row().height(height), children![lone]).unwrap();
    let node = row.layout_node();
    let reference = ComponentList::new(box_item(row));
    lay_out(node, 300.0, height);
    rect_of(&reference, text).expect("the lone leaf is drawn")
}

/// Showing or hiding the label and the value must not restyle them: as text leaves they fill the handle's height and are optically centred there, where dropped to the row's centre line they sat a pixel off.
#[test]
fn the_label_and_the_value_are_centred_in_the_handle_as_lone_text_leaves_of_its_height_are() {
    let tree = mounted(
        ScrubFieldProps::props()
            .label("Width")
            .value(signal(7.0))
            .build(),
    );
    let height = crate::test_support::drawn(Role::SpinButton)[0].height;

    assert_eq!(
        rect_of(&tree, "Width").expect("the label is drawn"),
        drawn_alone_in(height, "Width", |t| shared::quiet(t, 1.0)),
    );
    assert_eq!(
        rect_of(&tree, "7").expect("the value is drawn"),
        drawn_alone_in(height, "7", |t| shared::control_text(t, 1.0)),
    );
}
