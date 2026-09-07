use std::cell::RefCell;

use platform_core::ModifiersState;
use ui_core::{ComponentList, LayoutItem};

use super::*;
use crate::harness::{lay_out, moved, press, release, route};
use crate::test_support::fresh_layout_runtime;

const WINDOW: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 400.0,
    height: 300.0,
};

/// Every string the tree draws.
fn drawn_texts(tree: &ComponentList) -> Vec<String> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            renderer_core::DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

fn key(named: NamedKey) -> platform_core::Event {
    platform_core::Event::KeyPressed {
        key: Key::Named(named),
        modifiers: ModifiersState::default(),
    }
}

/// A menu of three rows with a separator in the middle and a disabled one, over a shared record of what was picked and whether it asked to be closed.
#[allow(clippy::type_complexity)]
fn menu() -> (ComponentList, Rc<RefCell<Vec<String>>>) {
    fresh_layout_runtime();
    let said: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let entries = {
        let (one, two, three) = (said.clone(), said.clone(), said.clone());
        vec![
            Entry::row("copiar", "ctrl+c", move || {
                one.borrow_mut().push("copiar".into())
            }),
            Entry::Separator,
            Entry::row("pegar", "ctrl+v", move || {
                two.borrow_mut().push("pegar".into())
            })
            .disabled(),
            Entry::Sub {
                label: "más".into(),
                entries: vec![Entry::row("hondo", "", move || {
                    three.borrow_mut().push("hondo".into())
                })],
            },
        ]
    };
    let closing = said.clone();
    let menu = context_menu(
        ContextMenuProps::props()
            .at((40.0, 30.0))
            .entries(entries)
            .on_close(Rc::new(move || closing.borrow_mut().push("cerrar".into())))
            .width(120.0)
            .within(WINDOW)
            .build(),
        Children::default(),
    )
    .unwrap();
    lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
    let tree = ComponentList::new(menu);
    (tree, said)
}

/// **The rows written where they are read.** A context menu's rows are heterogeneous and half of them are only there when they apply, which is an `if` around a row — and a `Vec` built with pushes in a function somewhere else is the one shape that cannot show that. The children register what they are, so the panel gets the same list it would have been handed.
#[test]
fn the_rows_may_be_children() {
    fresh_layout_runtime();
    let said: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let picked = said.clone();
    let rows = Children::new(move || {
        let picked = picked.clone();
        let mut slots = ui_core::Slots::new();
        slots.push(
            None,
            menu_row(
                MenuRowProps::props()
                    .label("copiar")
                    .hint("ctrl+c")
                    .on_select(Rc::new(move || picked.borrow_mut().push("copiar".into())))
                    .build(),
                Children::default(),
            )?,
        );
        slots.push(
            None,
            menu_separator(MenuSeparatorProps::props().build(), Children::default())?,
        );
        slots.push(
            None,
            menu_row(
                MenuRowProps::props().label("pegar").disabled(true).build(),
                Children::default(),
            )?,
        );
        Ok(slots)
    });
    let menu = context_menu(
        ContextMenuProps::props()
            .at((40.0, 30.0))
            .within(WINDOW)
            .build(),
        rows,
    )
    .unwrap();
    lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
    let mut tree = ComponentList::new(menu);

    assert!(
        drawn_texts(&tree).contains(&"copiar".to_string()),
        "the row the children declared is the row the panel drew: {:?}",
        drawn_texts(&tree)
    );

    route(&mut tree, &key(NamedKey::ArrowDown));
    route(&mut tree, &key(NamedKey::ArrowDown));
    route(&mut tree, &key(NamedKey::Enter));
    assert_eq!(
        *said.borrow(),
        vec!["copiar"],
        "a disabled row declared as a child is disabled in the panel: {:?}",
        said.borrow()
    );
}

/// **The arrows step over what cannot be picked.** A separator is not a row and a disabled one is there to be read, not chosen — a menu that stops on either is one where the keyboard counts lines instead of offering answers.
#[test]
fn the_keyboard_walks_the_rows_that_can_be_picked() {
    let (mut tree, said) = menu();

    route(&mut tree, &key(NamedKey::ArrowDown));
    route(&mut tree, &key(NamedKey::ArrowDown));
    route(&mut tree, &key(NamedKey::ArrowUp));
    route(&mut tree, &key(NamedKey::Enter));

    assert_eq!(
        *said.borrow(),
        vec!["cerrar", "copiar"],
        "{:?}",
        said.borrow()
    );
}

/// Rightwards into a submenu, leftwards out of it, and what is picked inside it is picked.
#[test]
fn a_submenu_opens_beside_its_row_and_answers_the_keyboard() {
    let (mut tree, said) = menu();
    route(&mut tree, &key(NamedKey::End));
    route(&mut tree, &key(NamedKey::ArrowRight));
    ui_core::relayout_if_dirty();

    assert!(
        drawn_texts(&tree).iter().any(|text| text == "hondo"),
        "el submenú no se abrió: {:?}",
        drawn_texts(&tree)
    );

    route(&mut tree, &key(NamedKey::ArrowDown));
    route(&mut tree, &key(NamedKey::Enter));
    assert_eq!(
        *said.borrow(),
        vec!["cerrar", "hondo"],
        "{:?}",
        said.borrow()
    );
}

/// **A submenu opens by being pointed at, and closes when the pointer moves off it.**
///
/// The two halves are one behaviour: what makes hovering back and forth across a list of submenus show one at a time is that an ordinary row closes whatever was open. Opening on the press alone made somebody click twice to reach a row they could already see the way to.
#[test]
fn pointing_at_a_submenu_opens_it() {
    let (mut tree, _said) = menu();
    assert!(
        !drawn_texts(&tree).iter().any(|text| text == "hondo"),
        "the submenu is closed, so its items must not be drawn: {:?}",
        drawn_texts(&tree)
    );

    // The fourth entry down, at the metrics the default style lays it out on: two rows and a separator above it, from a panel opened at y=30.
    route(&mut tree, &moved(60.0, 94.0));
    ui_core::relayout_if_dirty();
    assert!(
        drawn_texts(&tree).iter().any(|text| text == "hondo"),
        "el submenú no se abrió al apuntarlo: {:?}",
        drawn_texts(&tree)
    );

    route(&mut tree, &moved(60.0, 45.0));
    ui_core::relayout_if_dirty();
    assert!(
        !drawn_texts(&tree).iter().any(|text| text == "hondo"),
        "salir de la fila no lo cerró: {:?}",
        drawn_texts(&tree)
    );
}

/// Escape is done with it, and so is a press anywhere off the panel — while a press *on* it is not.
#[test]
fn every_way_out_says_so_once() {
    let (mut tree, said) = menu();
    // On the row that is there to be read and not chosen: inside the panel, and not an answer.
    route(&mut tree, &press(60.0, 70.0));
    route(&mut tree, &release(60.0, 70.0));
    assert!(
        said.borrow().is_empty(),
        "una pulsación en el panel lo cerró"
    );

    route(&mut tree, &press(380.0, 280.0));
    route(&mut tree, &release(380.0, 280.0));
    assert_eq!(*said.borrow(), vec!["cerrar"], "el clic fuera no lo cerró");

    said.borrow_mut().clear();
    route(&mut tree, &key(NamedKey::Escape));
    assert_eq!(*said.borrow(), vec!["cerrar"], "escape no lo cerró");
}
