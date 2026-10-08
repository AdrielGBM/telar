use std::cell::Cell;

use super::*;
use crate::{IconButtonProps, icon_button};
use telar::{AvailableSpace, ComponentList, Container, Event, Slots, compute_layout};

struct Bar {
    tree: ComponentList,
    fired: Rc<[Cell<u32>; 3]>,
}

fn bar(disabled: [bool; 3]) -> Bar {
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let fired: Rc<[Cell<u32>; 3]> = Rc::new(Default::default());
    let items = fired.clone();
    let children = Children::new(move || {
        let mut slots = Slots::new();
        for (index, off) in disabled.into_iter().enumerate() {
            let sink = items.clone();
            let item = icon_button(
                IconButtonProps::props()
                    .icon("lucide:search")
                    .label(format!("Item {index}"))
                    .disabled(off)
                    .on_press(Rc::new(move || sink[index].set(sink[index].get() + 1)))
                    .build(),
                Children::default(),
            )
            .unwrap();
            slots.push(None, item);
        }
        Ok(slots)
    });
    let bar = toolbar(ToolbarProps::props().label("Tools").build(), children).unwrap();
    let root = Container::new(
        LayoutStyle::new().flex_row().width(400.0).height(100.0),
        vec![bar],
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    let tree = ComponentList::new(box_item(root));
    let _ = tree.commands();
    Bar { tree, fired }
}

fn key(named: NamedKey) -> Event {
    Event::KeyPressed {
        key: Key::Named(named),
        modifiers: telar::ModifiersState::default(),
    }
}

impl Bar {
    fn press(&mut self, named: NamedKey) {
        self.tree.on_event(&key(named));
    }

    fn activated(&mut self) -> usize {
        self.press(NamedKey::Enter);
        let at = self.fired.iter().position(|count| count.get() > 0);
        self.fired.iter().for_each(|count| count.set(0));
        at.expect("an item fired")
    }
}

#[test]
fn the_whole_toolbar_is_one_tab_stop() {
    let _bar = bar([false; 3]);

    focus::focus_next();
    let entered = focus::current().expect("Tab enters the toolbar");
    focus::focus_next();
    assert_eq!(
        focus::current(),
        Some(entered),
        "and there is no second stop to reach"
    );
}

#[test]
fn the_toolbar_is_exposed_once_with_its_axis() {
    let _bar = bar([false; 3]);

    let toolbars: Vec<_> = focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::Toolbar)
        .collect();
    assert_eq!(toolbars.len(), 1);
    assert_eq!(toolbars[0].orientation, Some(Orientation::Horizontal));
}

#[test]
fn enter_presses_the_first_item_on_arrival() {
    let mut bar = bar([false; 3]);

    focus::focus_next();
    assert_eq!(bar.activated(), 0);
}

#[test]
fn the_arrows_move_between_items_and_wrap() {
    let mut bar = bar([false; 3]);
    focus::focus_next();

    bar.press(NamedKey::ArrowRight);
    assert_eq!(bar.activated(), 1);
    bar.press(NamedKey::ArrowRight);
    bar.press(NamedKey::ArrowRight);
    assert_eq!(bar.activated(), 0);
    bar.press(NamedKey::ArrowLeft);
    assert_eq!(bar.activated(), 2);
}

#[test]
fn home_and_end_jump_to_the_ends() {
    let mut bar = bar([false; 3]);
    focus::focus_next();

    bar.press(NamedKey::End);
    assert_eq!(bar.activated(), 2);
    bar.press(NamedKey::Home);
    assert_eq!(bar.activated(), 0);
}

#[test]
fn a_disabled_item_is_stepped_over() {
    let mut bar = bar([false, true, false]);
    focus::focus_next();

    bar.press(NamedKey::ArrowRight);
    assert_eq!(bar.activated(), 2);
}

#[test]
fn a_disabled_first_item_hands_the_cursor_to_the_first_enabled_one() {
    let mut bar = bar([true, false, false]);
    focus::focus_next();

    assert_eq!(bar.activated(), 1);
    bar.press(NamedKey::Home);
    assert_eq!(bar.activated(), 1);
}

#[test]
fn space_presses_the_current_item_like_enter() {
    let mut bar = bar([false; 3]);
    focus::focus_next();

    bar.press(NamedKey::ArrowRight);
    bar.press(NamedKey::Space);
    assert_eq!(bar.fired[1].get(), 1);
}

#[test]
fn keys_the_toolbar_does_not_keep_are_left_alone() {
    let mut bar = bar([false; 3]);
    focus::focus_next();

    bar.press(NamedKey::ArrowDown);
    assert!(bar.fired.iter().all(|count| count.get() == 0));
}

impl Bar {
    /// The name of the item a focused toolbar points a reader at, or `None` when it points at none.
    fn pointed(&self) -> Option<String> {
        let nodes = ui_core::accessibility::snapshot(&self.tree.commands());
        let toolbar = nodes
            .iter()
            .find(|node| node.role == Role::Toolbar && node.focused)?;
        let item = toolbar.active_descendant?;
        nodes
            .iter()
            .find(|node| node.id == Some(item))
            .map(|node| node.name.clone())
    }
}

#[test]
fn a_reader_is_pointed_at_the_item_the_cursor_is_on() {
    let mut bar = bar([false, true, false]);
    focus::focus_next();

    assert_eq!(bar.pointed().as_deref(), Some("Item 0"));
    bar.press(NamedKey::ArrowRight);
    assert_eq!(
        bar.pointed().as_deref(),
        Some("Item 2"),
        "the disabled item the cursor stepped over is not pointed at"
    );
    bar.press(NamedKey::Home);
    assert_eq!(bar.pointed().as_deref(), Some("Item 0"));
}

#[test]
fn an_item_is_announced_as_a_button_and_is_no_tab_stop_of_its_own() {
    let _bar = bar([false; 3]);

    let items: Vec<_> = focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::Button)
        .collect();
    assert_eq!(items.len(), 3);
    focus::focus_next();
    let toolbar = focus::current();
    focus::focus_next();
    assert_eq!(focus::current(), toolbar, "Tab never stops on an item");
}

#[test]
fn right_to_left_swaps_which_arrow_moves_forward() {
    let mut bar = bar([false; 3]);
    focus::focus_next();

    telar::set_direction(Direction::Rtl);
    bar.press(NamedKey::ArrowLeft);
    let forward = bar.activated();
    bar.press(NamedKey::ArrowRight);
    let back = bar.activated();
    telar::set_direction(Direction::Ltr);
    assert_eq!(
        forward, 1,
        "left moves forward when the toolbar reads right to left"
    );
    assert_eq!(back, 0, "and right moves back");
}

#[test]
fn a_vertical_toolbar_keeps_up_and_down_whatever_the_direction() {
    crate::test_support::fresh_layout_runtime();
    focus::clear();
    let fired: Rc<[Cell<u32>; 2]> = Rc::new(Default::default());
    let items = fired.clone();
    let children = Children::new(move || {
        let mut slots = Slots::new();
        for index in 0..2 {
            let sink = items.clone();
            let item = icon_button(
                IconButtonProps::props()
                    .icon("lucide:search")
                    .label(format!("Item {index}"))
                    .on_press(Rc::new(move || sink[index].set(sink[index].get() + 1)))
                    .build(),
                Children::default(),
            )
            .unwrap();
            slots.push(None, item);
        }
        Ok(slots)
    });
    let item = toolbar(
        ToolbarProps::props().label("Tools").vertical(true).build(),
        children,
    )
    .unwrap();
    crate::harness::lay_out(item.layout_node(), 100.0, 400.0);
    let mut tree = ComponentList::new(item);
    let _ = tree.commands();
    focus::focus_next();

    telar::set_direction(Direction::Rtl);
    tree.on_event(&key(NamedKey::ArrowDown));
    tree.on_event(&key(NamedKey::Enter));
    telar::set_direction(Direction::Ltr);
    assert_eq!(fired[1].get(), 1, "down still moves forward");
}
