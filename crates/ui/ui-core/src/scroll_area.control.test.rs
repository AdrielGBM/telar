use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{Event, Key, ModifiersState, NamedKey, PointerButton, PointerSource};
use renderer_core::{DrawCommand, RectStyle, Role};

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::layout_item::{LayoutItem, box_item};
use crate::styled_container::StyledContainer;

struct AsADocument(bool);

impl AsADocument {
    fn new() -> Self {
        Self(ui_tree::set_element_capture(true))
    }
}

impl Drop for AsADocument {
    fn drop(&mut self) {
        ui_tree::set_element_capture(self.0);
    }
}

fn row(height: f32) -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().width(80.0).height(height),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .presented(Role::TreeItem)
}

fn scroll(content_height: f32) -> LayoutScrollArea {
    let rows = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(row(content_height))],
    )
    .unwrap();
    LayoutScrollArea::new(
        LayoutStyle::new().width(200.0).height(100.0),
        box_item(rows),
    )
    .unwrap()
}

fn mounted(area: LayoutScrollArea) -> ui_tree::ComponentList {
    compute_layout(
        area.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    ui_tree::ComponentList::new(area)
}

fn press(key: NamedKey) -> Event {
    Event::KeyPressed {
        key: Key::Named(key),
        modifiers: ModifiersState::default(),
        unmodified: None,
    }
}

fn elements(tree: &ui_tree::ComponentList) -> Vec<Arc<renderer_core::Element>> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::PushElement { element } => Some(element.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_control_is_a_stop_whose_rows_do_not_take_the_stop_away() {
    reset_layout_runtime();
    focus::clear();
    let area = scroll(1000.0).control(Role::Tree);
    let id = area.focus_id().expect("a control holds focus by an id");
    let _tree = mounted(area);

    focus::focus_next();
    assert_eq!(focus::current(), Some(id));
    assert!(
        focus::exposed()
            .iter()
            .any(|e| e.id == id && e.role == Role::Tree),
        "and a reader is told what it is"
    );
    focus::clear();
}

#[test]
fn a_control_is_a_stop_when_its_content_fits() {
    reset_layout_runtime();
    focus::clear();
    let area = scroll(40.0).control(Role::Tree);
    let id = area.focus_id();
    let _tree = mounted(area);

    focus::focus_next();
    assert_eq!(focus::current(), id);
    focus::clear();
}

#[test]
fn the_scroll_area_that_is_not_a_control_has_no_focus_id_of_its_own_to_give() {
    reset_layout_runtime();
    assert_eq!(scroll(1000.0).focus_id(), None);
}

#[test]
fn a_control_is_one_element_that_has_the_role_scrolls_and_is_a_tab_stop() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let tree = mounted(scroll(1000.0).control(Role::Tree));

    let all = elements(&tree);
    let roles: Vec<Role> = all.iter().map(|e| e.semantics.role).collect();
    assert_eq!(roles.iter().filter(|r| **r == Role::Tree).count(), 1);
    assert!(
        !roles.contains(&Role::ScrollArea),
        "no scroll area is nested in the control"
    );
    let element = &all[0];
    assert_eq!(element.semantics.role, Role::Tree);
    assert!(element.scrolls);
    assert!(element.semantics.focusable.is_some_and(|f| f.tab_stop));
    focus::clear();
}

#[test]
fn a_plain_scroll_area_holding_a_stop_stays_a_scroll_area_without_a_stop_of_its_own() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let button = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(1000.0),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::Button);
    let rows = Container::new(LayoutStyle::new().flex_column(), vec![box_item(button)]).unwrap();
    let tree = mounted(
        LayoutScrollArea::new(
            LayoutStyle::new().width(200.0).height(100.0),
            box_item(rows),
        )
        .unwrap(),
    );

    let all = elements(&tree);
    assert_eq!(all[0].semantics.role, Role::ScrollArea);
    assert!(all[0].scrolls);
    assert!(all[0].semantics.focusable.is_none());
    focus::clear();
}

#[test]
fn the_cursor_a_control_points_at_reaches_the_element() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let target = row(30.0);
    let target_id = target.focus_id();
    let target_node = target.layout_node();
    let rows = Container::new(LayoutStyle::new().flex_column(), vec![box_item(target)]).unwrap();
    let area = LayoutScrollArea::new(
        LayoutStyle::new().width(200.0).height(100.0),
        box_item(rows),
    )
    .unwrap()
    .control(Role::Tree)
    .active_descendant(move || target_id);
    let tree = mounted(area);

    assert_eq!(
        elements(&tree)[0].semantics.active_descendant,
        Some(u64::from(target_node))
    );
    focus::clear();
}

fn viewed(content_height: f32) -> (LayoutScrollArea, ScrollViewport) {
    let mut viewport = None;
    let area =
        LayoutScrollArea::new_with(LayoutStyle::new().width(200.0).height(100.0), |handed| {
            viewport = Some(handed);
            let rows = Container::new(
                LayoutStyle::new().flex_column(),
                vec![box_item(row(content_height))],
            )?;
            Ok(box_item(rows))
        })
        .unwrap();
    (area, viewport.unwrap())
}

#[test]
fn a_key_the_control_answers_does_not_also_scroll_and_one_it_does_not_scrolls() {
    reset_layout_runtime();
    focus::clear();
    let heard = Rc::new(RefCell::new(Vec::new()));
    let sink = heard.clone();
    let (area, viewport) = viewed(1000.0);
    let area = area.control(Role::Tree).on_focused_key(move |key: &Key| {
        sink.borrow_mut().push(key.clone());
        matches!(key, Key::Named(NamedKey::ArrowDown))
    });
    let id = area.focus_id().unwrap();
    let mut tree = mounted(area);

    tree.on_event(&press(NamedKey::ArrowDown));
    assert!(
        heard.borrow().is_empty(),
        "an unfocused control hears no keys"
    );

    focus::request(id);
    tree.on_event(&press(NamedKey::ArrowDown));
    assert_eq!(heard.borrow().len(), 1);
    assert_eq!(
        viewport.peek_offset().1,
        0.0,
        "the answered key moved the cursor, not the view"
    );

    tree.on_event(&press(NamedKey::End));
    assert_eq!(
        viewport.peek_offset().1,
        900.0,
        "a key it leaves alone scrolls"
    );
    focus::clear();
}

#[test]
fn a_control_tells_when_it_gains_and_loses_focus() {
    reset_layout_runtime();
    focus::clear();
    let changes = Rc::new(RefCell::new(Vec::new()));
    let sink = changes.clone();
    let area = scroll(1000.0)
        .control(Role::Tree)
        .on_focus(move |now| sink.borrow_mut().push(now));
    let id = area.focus_id().unwrap();
    let _tree = mounted(area);

    focus::request(id);
    focus::clear();
    assert_eq!(*changes.borrow(), vec![true, false]);
}

#[test]
fn a_press_in_a_control_focuses_it_and_a_press_in_a_presented_one_does_not() {
    reset_layout_runtime();
    focus::clear();
    let pressed = Event::PointerPressed {
        x: 20.0,
        y: 20.0,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    };
    let area = scroll(1000.0).control(Role::Tree);
    let id = area.focus_id();
    let mut tree = mounted(area);
    tree.on_event(&pressed);
    assert_eq!(focus::current(), id);
    focus::clear();

    let mut presented = mounted(scroll(1000.0).presented(Role::ListBox));
    presented.on_event(&pressed);
    assert_eq!(focus::current(), None);
}

#[test]
fn a_primary_scroll_cannot_be_a_control() {
    reset_layout_runtime();
    focus::clear();
    let area = scroll(1000.0).into_primary().control(Role::Tree);
    assert_eq!(area.focus_id(), None);
}

/// The list a combobox opens is announced and scrolls, but the keyboard stays on the combobox: no Tab stop, and a press in it leaves focus alone.
#[test]
fn a_presented_list_is_announced_but_neither_tab_nor_a_press_focuses_it() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let area = scroll(1000.0).presented(Role::ListBox);
    let id = area
        .focus_id()
        .expect("a presented list is announced by an id");
    let mut tree = mounted(area);

    assert!(
        focus::exposed()
            .iter()
            .any(|e| e.id == id && e.role == Role::ListBox),
        "a reader is told what it is"
    );
    let element = &elements(&tree)[0];
    assert_eq!(element.semantics.role, Role::ListBox);
    assert!(element.scrolls);
    assert!(element.semantics.focusable.is_some_and(|f| !f.tab_stop));

    focus::focus_next();
    assert_eq!(focus::current(), None, "Tab does not stop on it");
    tree.on_event(&Event::PointerPressed {
        x: 20.0,
        y: 20.0,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    });
    assert_eq!(focus::current(), None, "a press does not focus it");
}

/// A combobox names the list it opens, wherever that list is, and its cursor may rest on a row of that list.
#[test]
fn a_control_names_the_list_it_controls_and_points_into_it() {
    reset_layout_runtime();
    focus::clear();
    let _document = AsADocument::new();
    let target = row(30.0);
    let (row_id, row_node) = (target.focus_id(), target.layout_node());
    let rows = Container::new(LayoutStyle::new().flex_column(), vec![box_item(target)]).unwrap();
    let list = LayoutScrollArea::new(
        LayoutStyle::new().width(200.0).height(100.0),
        box_item(rows),
    )
    .unwrap()
    .presented(Role::ListBox);
    let (list_id, list_node) = (list.focus_id(), list.layout_node());
    let open = reactive_core::signal(true);
    let combobox = StyledContainer::new(
        LayoutStyle::new().width(200.0).height(30.0),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .control(Role::ComboBox)
    .controls(move || open.get().then_some(list_id).flatten())
    .active_descendant(move || open.get().then_some(row_id).flatten());
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(combobox), box_item(list)],
    )
    .unwrap();
    let tree = mounted_root(root);

    let combobox = &elements(&tree)[1];
    assert_eq!(combobox.semantics.role, Role::ComboBox);
    assert_eq!(combobox.semantics.controls, Some(u64::from(list_node)));
    assert_eq!(
        combobox.semantics.active_descendant,
        Some(u64::from(row_node))
    );

    open.set(false);
    let combobox = &elements(&tree)[1];
    assert_eq!(combobox.semantics.controls, None, "a closed one names none");
    assert_eq!(combobox.semantics.active_descendant, None);
    focus::clear();
}

fn mounted_root(root: Container) -> ui_tree::ComponentList {
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(200.0),
    )
    .unwrap();
    ui_tree::ComponentList::new(root)
}
