use std::cell::{Cell, RefCell};
use std::rc::Rc;

use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{Key, ModifiersState, NamedKey};
use reactive_core::signal;
use renderer_core::RectStyle;
use ui_tree::{Component, EventResult};

use super::*;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::layout_item::{LayoutItem, box_item};
use crate::styled_container::StyledContainer;
use focus::FocusId;

fn key(named: NamedKey) -> Event {
    Event::KeyPressed {
        key: Key::Named(named),
        modifiers: ModifiersState::default(),
        unmodified: None,
    }
}

fn unrouted(event: &Event) {
    panic!("an action on a control of this surface travels no tree: {event:?}")
}

fn laid_out(item: &dyn LayoutItem) {
    compute_layout(
        item.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
}

struct Listed {
    list: StyledContainer,
    id: FocusId,
    rows: Vec<FocusId>,
    pressed: Rc<RefCell<Vec<usize>>>,
}

/// A list box of three options, focused, that walks its rows with Down while it keeps focus and points a reader at the row its cursor is on; the presses its rows take are logged.
fn focused_list() -> Listed {
    reset_layout_runtime();
    focus::clear();
    let cursor = signal(0usize);
    let pressed: Rc<RefCell<Vec<usize>>> = Rc::default();
    let mut rows = Vec::new();
    let mut built: Vec<Box<dyn LayoutItem>> = Vec::new();
    for index in 0..3 {
        let log = pressed.clone();
        let row = StyledContainer::new(
            LayoutStyle::new().width(80.0).height(20.0),
            |_r| RectStyle::default(),
            vec![],
        )
        .unwrap()
        .presented(Role::Option, move || cursor.set(index))
        .on_press(move || log.borrow_mut().push(index));
        rows.push(row.focus_id().expect("a presented row has an id"));
        built.push(box_item(row));
    }
    let pointed = rows.clone();
    let list = StyledContainer::new(
        LayoutStyle::new().flex_column(),
        |_r| RectStyle::default(),
        built,
    )
    .unwrap()
    .control(Role::ListBox)
    .active_descendant(move || pointed.get(cursor.get()).copied())
    .on_focused_key(move |key: &Key| match key {
        Key::Named(NamedKey::ArrowDown) => {
            cursor.set((cursor.peek() + 1).min(2));
            true
        }
        _ => false,
    });
    laid_out(&list);
    let id = list.focus_id().expect("a control has an id");
    focus::focus_next();
    assert_eq!(focus::current(), Some(id), "the list is the one stop");
    Listed {
        list,
        id,
        rows,
        pressed,
    }
}

#[test]
fn a_reader_activating_a_row_presses_it_and_the_list_keeps_the_keyboard() {
    let Listed {
        mut list,
        id,
        rows,
        pressed,
    } = focused_list();

    perform_action(rows[1], true, unrouted);
    assert_eq!(*pressed.borrow(), [1], "the row's own press ran, once");
    assert_eq!(focus::current(), Some(id), "focus stayed on the list");
    assert_eq!(
        focus::active_descendant_state(id),
        Some(rows[1]),
        "whose cursor is on the row"
    );

    assert_eq!(
        list.on_event(&key(NamedKey::ArrowDown)),
        EventResult::Handled
    );
    assert_eq!(
        focus::active_descendant_state(id),
        Some(rows[2]),
        "the next arrow carries on from there"
    );
}

#[test]
fn a_reader_moving_to_a_row_moves_the_list_s_cursor_and_presses_nothing() {
    let Listed {
        list: _list,
        id,
        rows,
        pressed,
    } = focused_list();

    perform_action(rows[2], false, unrouted);
    assert_eq!(focus::current(), Some(id));
    assert_eq!(focus::active_descendant_state(id), Some(rows[2]));
    assert!(pressed.borrow().is_empty());
}

#[test]
fn a_presented_row_never_holds_focus_however_it_is_asked_for() {
    let Listed {
        list: _list,
        id,
        rows,
        ..
    } = focused_list();
    focus::request(rows[0]);
    focus::request_from_pointer(rows[1]);
    assert_eq!(focus::current(), Some(id));
}

fn counted_button(disabled: bool) -> (StyledContainer, Rc<Cell<u32>>) {
    reset_layout_runtime();
    focus::clear();
    let presses = Rc::new(Cell::new(0));
    let sink = presses.clone();
    let button = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(30.0),
        |_r| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Button)
    .disabled(move || disabled)
    .on_press(move || sink.set(sink.get() + 1));
    laid_out(&button);
    (button, presses)
}

#[test]
fn a_reader_activating_a_button_presses_it_once_and_gives_it_focus() {
    let (mut button, presses) = counted_button(false);
    let id = button.focus_id().expect("a control has an id");
    perform_action(id, true, |event| {
        button.on_event(event);
    });
    assert_eq!(presses.get(), 1);
    assert_eq!(focus::current(), Some(id));
}

#[test]
fn a_reader_cannot_activate_a_disabled_control() {
    let (button, presses) = counted_button(true);
    perform_action(
        button.focus_id().expect("a control has an id"),
        true,
        unrouted,
    );
    assert_eq!(presses.get(), 0);
}
