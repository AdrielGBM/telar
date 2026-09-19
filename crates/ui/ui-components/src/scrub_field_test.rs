use std::cell::Cell;
use std::rc::Rc;

use platform_core::{Key, NamedKey};
use reactive_core::{RwSignal, Transaction, signal};
use ui_core::ComponentList;

use super::*;
use crate::harness::{hold, key_with, lay_out, moved, named, press, release, route};
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
    ui_core::reset_keyboard();
    ui_core::focus::clear();
}

fn field(start: f32) -> Field {
    fresh();
    let value = signal(start);
    field_over(value, Transaction::new(value), -1000.0, 1000.0)
}

impl Field {
    fn event(&mut self, event: platform_core::Event) {
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
