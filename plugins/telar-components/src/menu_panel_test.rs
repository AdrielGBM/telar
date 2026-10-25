use std::cell::RefCell;

use telar::testing::{
    Scoped, centre, key_with, lay_out, moved, named, press, release, route, scoped,
};
use telar::{
    ComponentList, Container, ModifiersState, RectStyle, Slots, ThemeTokens, relayout_if_dirty,
};

use super::*;
use crate::list::{GroupProps, ItemProps, ListTokens, SeparatorProps, group, item, separator};
use crate::menu::{ContextMenuProps, MenuProps, context_menu, is_context_menu_key, menu};
use crate::test_support::{drawn, fresh_layout_runtime};
use telar::focus::Role;

const WINDOW: Rect = Rect {
    x: 0.0,
    y: 0.0,
    width: 400.0,
    height: 300.0,
};

/// What a test menu is written as: rows that say their name into a shared record when picked, rules, headings and submenus.
#[derive(Clone)]
enum Line {
    Row(&'static str),
    Hinted(&'static str, &'static str),
    Disabled(&'static str),
    Rule,
    Heading(&'static str),
    Sub(&'static str, Vec<Line>),
}

type Said = Rc<RefCell<Vec<String>>>;

fn written(lines: Vec<Line>, said: &Said) -> Children {
    let said = said.clone();
    Children::new(move || {
        let mut slots = Slots::new();
        for line in &lines {
            slots.push(None, built(line, &said)?);
        }
        Ok(slots)
    })
}

fn built(line: &Line, said: &Said) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let picked = |name: &'static str| {
        let said = said.clone();
        Rc::new(move || said.borrow_mut().push(name.to_string())) as Rc<dyn Fn()>
    };
    match line.clone() {
        Line::Row(name) => item(
            ItemProps::props()
                .label(name)
                .on_press(picked(name))
                .build(),
            Children::default(),
        ),
        Line::Hinted(name, hint) => item(
            ItemProps::props()
                .label(name)
                .hint(hint)
                .on_press(picked(name))
                .build(),
            Children::default(),
        ),
        Line::Disabled(name) => item(
            ItemProps::props()
                .label(name)
                .disabled(true)
                .on_press(picked(name))
                .build(),
            Children::default(),
        ),
        Line::Rule => separator(SeparatorProps::props().build(), Children::default()),
        Line::Heading(name) => group(GroupProps::props().label(name).build(), Children::default()),
        Line::Sub(name, lines) => submenu(
            SubmenuProps::props().label(name).build(),
            written(lines, said),
        ),
    }
}

/// copiar · rule · [disabled] pegar · más ▸ hondo: the menu most of these tests open.
fn sample() -> Vec<Line> {
    vec![
        Line::Hinted("copiar", "ctrl+c"),
        Line::Rule,
        Line::Disabled("pegar"),
        Line::Sub("más", vec![Line::Row("hondo")]),
    ]
}

fn rows(count: usize) -> Vec<Line> {
    const NAMES: [&str; 30] = [
        "fila 0", "fila 1", "fila 2", "fila 3", "fila 4", "fila 5", "fila 6", "fila 7", "fila 8",
        "fila 9", "fila 10", "fila 11", "fila 12", "fila 13", "fila 14", "fila 15", "fila 16",
        "fila 17", "fila 18", "fila 19", "fila 20", "fila 21", "fila 22", "fila 23", "fila 24",
        "fila 25", "fila 26", "fila 27", "fila 28", "fila 29",
    ];
    NAMES[..count].iter().map(|name| Line::Row(name)).collect()
}

fn settle(tree: &ComponentList) {
    for _ in 0..3 {
        relayout_if_dirty();
        let _ = tree.commands();
    }
}

/// A context menu of `lines` asked for at `at` in [`WINDOW`], laid out and drawn, over a record of what was picked and whether it asked to be closed.
fn opened_at(at: (f32, f32), lines: Vec<Line>) -> (Scoped<ComponentList>, Said) {
    fresh_layout_runtime();
    focus::clear();
    let said: Said = Rc::default();
    let closing = said.clone();
    let tree = scoped(|| {
        let menu = context_menu(
            ContextMenuProps::props()
                .at(at)
                .on_close(Rc::new(move || closing.borrow_mut().push("cerrar".into())))
                .width(120.0)
                .within(WINDOW)
                .build(),
            written(lines, &said),
        )
        .unwrap();
        lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
        ComponentList::new(menu)
    });
    settle(&tree);
    (tree, said)
}

fn opened() -> (Scoped<ComponentList>, Said) {
    opened_at((40.0, 30.0), sample())
}

/// Every string the tree draws.
fn drawn_texts(tree: &ComponentList) -> Vec<String> {
    tree.commands()
        .iter()
        .filter_map(|command| match command {
            telar::DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

fn shows(tree: &ComponentList, text: &str) -> bool {
    drawn_texts(tree).iter().any(|drawn| drawn == text)
}

fn panels() -> Vec<Rect> {
    drawn(Role::Menu)
}

fn bottom(rect: Rect) -> f32 {
    rect.y + rect.height
}

fn lists() -> Vec<FocusId> {
    focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::Menu)
        .map(|exposed| exposed.id)
        .collect()
}

fn exposed_named(role: Role, rect: Rect) -> focus::Exposed {
    focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == role && telar::visible_rect(exposed.node) == Some(rect))
        .expect("the control is exposed")
}

/// **The arrows step over what cannot be picked.** A separator is not a row and a disabled one is there to be read, not chosen — a menu that stops on either is one where the keyboard counts lines instead of offering answers.
#[test]
fn the_keyboard_walks_the_rows_that_can_be_picked() {
    let (mut tree, said) = opened();

    route(&mut tree, &named(NamedKey::ArrowDown));
    route(&mut tree, &named(NamedKey::ArrowDown));
    route(&mut tree, &named(NamedKey::ArrowUp));
    route(&mut tree, &named(NamedKey::Enter));

    assert_eq!(
        *said.borrow(),
        vec!["cerrar", "copiar"],
        "{:?}",
        said.borrow()
    );
}

/// Rightwards into a submenu, with the cursor on its first row as a menu opened from the keyboard puts it; what is picked inside it is picked, and closes the whole menu.
#[test]
fn a_submenu_opens_beside_its_row_and_answers_the_keyboard() {
    let (mut tree, said) = opened();
    route(&mut tree, &named(NamedKey::End));
    route(&mut tree, &named(NamedKey::ArrowRight));
    settle(&tree);
    assert!(
        shows(&tree, "hondo"),
        "el submenú no se abrió: {:?}",
        drawn_texts(&tree)
    );

    route(&mut tree, &named(NamedKey::Enter));
    assert_eq!(
        *said.borrow(),
        vec!["cerrar", "hondo"],
        "{:?}",
        said.borrow()
    );
}

/// Enter and Space on a submenu's row open it as the rightward arrow does: a row that does nothing when it is committed is a row the keyboard cannot use.
#[test]
fn enter_on_a_submenu_s_row_opens_it_at_its_first_row() {
    for key in [NamedKey::Enter, NamedKey::Space] {
        let (mut tree, said) = opened();
        route(&mut tree, &named(NamedKey::End));
        route(&mut tree, &named(key.clone()));
        settle(&tree);
        assert!(shows(&tree, "hondo"), "{key:?} opened it");
        route(&mut tree, &named(NamedKey::Enter));
        assert_eq!(*said.borrow(), vec!["cerrar", "hondo"], "{key:?}");
    }
}

/// The leftward arrow and Escape each close the submenu alone, and leave the menu it opened from open, with its cursor where it was.
#[test]
fn the_leftward_arrow_and_escape_close_only_the_submenu() {
    for key in [NamedKey::ArrowLeft, NamedKey::Escape] {
        let (mut tree, said) = opened();
        route(&mut tree, &named(NamedKey::End));
        route(&mut tree, &named(NamedKey::ArrowRight));
        settle(&tree);
        route(&mut tree, &named(key.clone()));
        settle(&tree);
        assert!(!shows(&tree, "hondo"), "{key:?} closed the submenu");
        assert!(shows(&tree, "copiar"), "and left the menu open");
        assert!(
            said.borrow().is_empty(),
            "{key:?} closed nothing else: {:?}",
            said.borrow()
        );
        route(&mut tree, &named(NamedKey::ArrowRight));
        settle(&tree);
        assert!(
            shows(&tree, "hondo"),
            "the cursor stayed on the submenu's row"
        );
    }
}

/// A leftward arrow on the first panel is not a way out: there is nothing to its left to go back to.
#[test]
fn the_leftward_arrow_does_nothing_on_the_first_panel() {
    let (mut tree, said) = opened();
    route(&mut tree, &named(NamedKey::ArrowLeft));
    settle(&tree);
    assert!(shows(&tree, "copiar"));
    assert!(said.borrow().is_empty());
}

/// **A submenu opens by being pointed at, and closes when the pointer moves to another row.**
///
/// The two halves are one behaviour: what makes hovering back and forth across a list of submenus show one at a time is that an ordinary row closes whatever was open. Opening on the press alone made somebody click twice to reach a row they could already see the way to.
#[test]
fn pointing_at_a_submenu_opens_it() {
    let (mut tree, _said) = opened();
    assert!(
        !shows(&tree, "hondo"),
        "the submenu is closed, so its rows are not drawn"
    );

    let (x, y) = centre(drawn(Role::MenuItem)[2]);
    route(&mut tree, &moved(x, y));
    settle(&tree);
    assert!(shows(&tree, "hondo"), "el submenú no se abrió al apuntarlo");

    let (x, y) = centre(drawn(Role::MenuItem)[0]);
    route(&mut tree, &moved(x, y));
    settle(&tree);
    assert!(!shows(&tree, "hondo"), "salir a otra fila no lo cerró");
}

/// The pointer resting on a row puts the keyboard's cursor there, so the arrows go on from where the hand left them.
#[test]
fn the_pointer_moves_the_cursor() {
    let (mut tree, said) = opened_at((40.0, 30.0), rows(3));
    let (x, y) = centre(drawn(Role::MenuItem)[1]);
    route(&mut tree, &moved(x, y));
    settle(&tree);
    route(&mut tree, &named(NamedKey::ArrowDown));
    route(&mut tree, &named(NamedKey::Enter));
    assert_eq!(*said.borrow(), vec!["cerrar", "fila 2"]);
}

/// Escape is done with it, and so is a press anywhere off the panel — while a press *on* it is not.
#[test]
fn every_way_out_says_so_once() {
    let (mut tree, said) = opened();
    let disabled = drawn(Role::MenuItem)[1];
    let (x, y) = centre(disabled);
    route(&mut tree, &press(x, y));
    route(&mut tree, &release(x, y));
    assert!(
        said.borrow().is_empty(),
        "una pulsación en el panel lo cerró"
    );

    route(&mut tree, &press(380.0, 280.0));
    route(&mut tree, &release(380.0, 280.0));
    assert_eq!(*said.borrow(), vec!["cerrar"], "el clic fuera no lo cerró");

    said.borrow_mut().clear();
    route(&mut tree, &named(NamedKey::Escape));
    assert_eq!(*said.borrow(), vec!["cerrar"], "escape no lo cerró");
}

/// Type-ahead walks a context menu's rows as it walks a `menu`'s: both are the same panel.
#[test]
fn a_typed_letter_takes_the_cursor_to_the_row_it_names() {
    let (mut tree, said) = opened_at(
        (40.0, 30.0),
        vec![Line::Row("abrir"), Line::Row("borrar"), Line::Row("buscar")],
    );
    telar::testing::reset_keyboard();
    route(
        &mut tree,
        &key_with(Key::Char('b'), ModifiersState::default()),
    );
    route(
        &mut tree,
        &key_with(Key::Char('u'), ModifiersState::default()),
    );
    route(&mut tree, &named(NamedKey::Enter));
    telar::testing::reset_keyboard();
    assert_eq!(*said.borrow(), vec!["cerrar", "buscar"]);
}

/// Thirty rows are taller than the window; the panel used to be pushed up to its top edge and still run off the bottom.
#[test]
fn a_long_menu_is_capped_to_the_window_and_scrolls_to_the_keyboard() {
    let (mut tree, _) = opened_at((40.0, 30.0), rows(30));
    let panel = panels()[0];
    assert_eq!(panel.y, 30.0 + crate::shared::panel_pad(), "at the point");
    assert!(
        bottom(panel) <= WINDOW.height,
        "inside the window: {panel:?}"
    );

    route(&mut tree, &named(NamedKey::End));
    settle(&tree);
    let last = *drawn(Role::MenuItem)
        .last()
        .expect("the rows are announced");
    assert!(
        last.y >= panel.y - 0.5 && bottom(last) <= bottom(panel) + 0.5,
        "the highlighted row is scrolled into the panel: {last:?} in {panel:?}"
    );
}

#[test]
fn a_menu_asked_for_near_the_bottom_opens_above_the_point() {
    let point = (40.0, 280.0);
    let (tree, _) = opened_at(point, rows(3));
    let panel = panels()[0];
    assert!(
        bottom(panel) <= point.1,
        "above the point it was asked at: {panel:?}"
    );
    assert!(shows(&tree, "fila 0"), "and whole");
}

/// A submenu opens against the panel it hangs from, its first row level with the row that opened it.
#[test]
fn a_submenu_opens_flush_beside_its_panel_and_level_with_its_row() {
    let mut lines = rows(2);
    lines.push(Line::Sub("más", rows(3)));
    let (mut tree, _) = opened_at((40.0, 30.0), lines);
    route(&mut tree, &named(NamedKey::End));
    route(&mut tree, &named(NamedKey::ArrowRight));
    settle(&tree);

    let found = panels();
    assert_eq!(found.len(), 2, "the submenu opened: {found:?}");
    let pad = crate::shared::panel_pad();
    let (menu, sub) = (found[0], found[1]);
    assert_eq!(
        sub.x - pad,
        menu.x + menu.width + pad,
        "flush against the menu's edge"
    );
    let row = drawn(Role::MenuItem)[2];
    assert_eq!(
        sub.y, row.y,
        "its first row level with the row it opened from"
    );
}

/// Beside the right edge a submenu opens on the left of its row, and one low in the window is lifted to stay in it.
#[test]
fn a_submenu_opens_on_the_side_with_room_and_stays_in_the_window() {
    let mut lines = rows(4);
    lines.push(Line::Sub("más", rows(6)));
    let (mut tree, _) = opened_at((300.0, 120.0), lines);
    route(&mut tree, &named(NamedKey::End));
    route(&mut tree, &named(NamedKey::ArrowRight));
    settle(&tree);

    let found = panels();
    assert_eq!(found.len(), 2, "the submenu opened: {found:?}");
    let (menu, sub) = (found[0], found[1]);
    assert!(
        sub.x + sub.width <= menu.x,
        "left of the menu: {sub:?} beside {menu:?}"
    );
    assert!(bottom(sub) <= WINDOW.height, "inside the window: {sub:?}");
}

/// A submenu's row says it opens a menu and whether that menu is showing; the submenu is a menu of its own.
#[test]
fn a_submenu_s_row_says_it_opens_a_menu_and_whether_it_is_open() {
    let (mut tree, _) = opened();
    let row = drawn(Role::MenuItem)[2];
    let said = exposed_named(Role::MenuItem, row);
    assert_eq!(said.has_popup, Some(telar::HasPopup::Menu));
    assert_eq!(said.expanded, Some(false));
    assert_eq!(
        exposed_named(Role::MenuItem, drawn(Role::MenuItem)[0]).has_popup,
        None,
        "a plain row opens nothing"
    );

    route(&mut tree, &named(NamedKey::End));
    route(&mut tree, &named(NamedKey::ArrowRight));
    settle(&tree);
    assert_eq!(exposed_named(Role::MenuItem, row).expanded, Some(true));
    assert_eq!(lists().len(), 2, "the submenu is a menu");
}

/// A row with a tick is a checkbox of the menu, and one of a set of choices a radio, each telling a reader whether it is on.
#[test]
fn ticked_rows_say_what_they_are_and_whether_they_are_on() {
    fresh_layout_runtime();
    focus::clear();
    let rows = Children::new(|| {
        let mut slots = Slots::new();
        slots.push(
            None,
            item(
                ItemProps::props().label("Reglas").checked(true).build(),
                Children::default(),
            )?,
        );
        for (name, on) in [("Claro", false), ("Oscuro", true)] {
            slots.push(
                None,
                item(
                    ItemProps::props()
                        .label(name)
                        .checked(on)
                        .radio(true)
                        .build(),
                    Children::default(),
                )?,
            );
        }
        Ok(slots)
    });
    let menu = context_menu(ContextMenuProps::props().within(WINDOW).build(), rows).unwrap();
    lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
    let tree = ComponentList::new(menu);
    settle(&tree);
    let toggled = |role: Role| -> Vec<Option<bool>> {
        focus::exposed()
            .into_iter()
            .filter(|exposed| exposed.role == role)
            .map(|exposed| exposed.toggled)
            .collect()
    };
    assert_eq!(toggled(Role::MenuItemCheckBox), vec![Some(true)]);
    assert_eq!(toggled(Role::MenuItemRadio), vec![Some(false), Some(true)]);
}

/// Every row of a panel keeps the column a tick stands in once one row draws a tick, so the labels line up whether or not their row carries one.
#[test]
fn the_labels_line_up_when_one_row_carries_a_tick() {
    fresh_layout_runtime();
    focus::clear();
    let rows = Children::new(|| {
        let mut slots = Slots::new();
        slots.push(
            None,
            submenu(
                SubmenuProps::props().label("Modo").build(),
                Children::default(),
            )?,
        );
        slots.push(
            None,
            item(
                ItemProps::props().label("Reglas").checked(true).build(),
                Children::default(),
            )?,
        );
        Ok(slots)
    });
    let menu = context_menu(ContextMenuProps::props().within(WINDOW).build(), rows).unwrap();
    lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
    let tree = ComponentList::new(menu);
    settle(&tree);
    let left_of = |needle: &str| {
        tree.commands()
            .iter()
            .find_map(|command| match command {
                telar::DrawCommand::Text { text, rect, .. } if text.as_ref() == needle => {
                    Some(rect.x)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{needle:?} is drawn"))
    };
    assert_eq!(left_of("Modo"), left_of("Reglas"));
}

/// A submenu reports what is picked in it to its own `on_select`, by the row's place among its own rows.
#[test]
fn a_submenu_reports_its_own_picks() {
    fresh_layout_runtime();
    focus::clear();
    let picked: Rc<RefCell<Vec<u32>>> = Rc::default();
    let top: Rc<RefCell<Vec<u32>>> = Rc::default();
    let (sink, outer) = (picked.clone(), top.clone());
    let rows = Children::new(move || {
        let sink = sink.clone();
        let mut slots = Slots::new();
        slots.push(
            None,
            submenu(
                SubmenuProps::props()
                    .label("Modo")
                    .on_select(Rc::new(move |at| sink.borrow_mut().push(at)))
                    .build(),
                Children::new(|| {
                    let mut slots = Slots::new();
                    for name in ["Sistema", "Claro", "Oscuro"] {
                        slots.push(
                            None,
                            item(ItemProps::props().label(name).build(), Children::default())?,
                        );
                    }
                    Ok(slots)
                }),
            )?,
        );
        Ok(slots)
    });
    let menu = context_menu(
        ContextMenuProps::props()
            .within(WINDOW)
            .on_select(Rc::new(move |at| outer.borrow_mut().push(at)))
            .build(),
        rows,
    )
    .unwrap();
    lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
    let mut tree = ComponentList::new(menu);
    settle(&tree);
    route(&mut tree, &named(NamedKey::ArrowDown));
    route(&mut tree, &named(NamedKey::ArrowRight));
    settle(&tree);
    route(&mut tree, &named(NamedKey::End));
    route(&mut tree, &named(NamedKey::Enter));
    assert_eq!(*picked.borrow(), vec![2]);
    assert!(top.borrow().is_empty(), "the first panel picked nothing");
}

/// A row written outside any menu has nothing to open, and says so rather than vanishing.
#[test]
fn a_submenu_outside_a_menu_is_an_error() {
    fresh_layout_runtime();
    let built = submenu(
        SubmenuProps::props().label("Modo").build(),
        Children::default(),
    );
    assert!(built.is_err());
}

/// A button that had the keyboard, and a menu shown while `open` holds, the way an application hosts one: closing it is the caller taking it out of the tree.
fn hosted(open: RwSignal<bool>, lines: Vec<Line>) -> (ComponentList, FocusId) {
    fresh_layout_runtime();
    focus::clear();
    let said: Said = Rc::default();
    let button = StyledContainer::new(
        LayoutStyle::new().width(80.0).height(30.0),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(|| {})
    .control(Role::Button);
    let button_id = button.focus_id().expect("a control");
    let menu = telar::ReactiveList::new(
        move || vec![open.get()],
        |shown: &bool| *shown,
        move |shown| -> Result<Box<dyn LayoutItem>, LayoutError> {
            if !shown {
                return Ok(box_item(Container::new(LayoutStyle::new(), vec![])?));
            }
            context_menu(
                ContextMenuProps::props()
                    .at((40.0, 60.0))
                    .on_close(Rc::new(move || open.set(false)))
                    .width(120.0)
                    .within(WINDOW)
                    .build(),
                written(lines.clone(), &said),
            )
        },
        0.0,
    )
    .unwrap();
    let root = Container::new(
        LayoutStyle::new().flex_column(),
        vec![box_item(button), box_item(menu)],
    )
    .unwrap();
    lay_out(root.layout_node(), WINDOW.width, WINDOW.height);
    let tree = ComponentList::new(root);
    settle(&tree);
    focus::request(button_id);
    (tree, button_id)
}

/// A context menu takes the keyboard as it opens, hands it to a submenu as the keys go there and back as they return, and gives it back to whatever held it when the menu goes.
#[test]
fn the_keyboard_goes_into_the_menu_follows_its_submenus_and_comes_back() {
    let mut lines = rows(2);
    lines.push(Line::Sub("más", rows(2)));
    let open = telar::signal(false);
    let (mut tree, button) = hosted(open, lines);

    open.set(true);
    settle(&tree);
    let menu = lists()[0];
    assert_eq!(focus::current(), Some(menu), "into the menu");

    route(&mut tree, &named(NamedKey::End));
    route(&mut tree, &named(NamedKey::ArrowRight));
    settle(&tree);
    let opened = lists();
    assert_eq!(opened.len(), 2, "the submenu opened: {opened:?}");
    assert_eq!(focus::current(), Some(opened[1]), "into the submenu");

    route(&mut tree, &named(NamedKey::ArrowLeft));
    settle(&tree);
    assert_eq!(focus::current(), Some(menu), "back to the menu");

    route(&mut tree, &named(NamedKey::ArrowRight));
    settle(&tree);
    route(&mut tree, &named(NamedKey::Escape));
    settle(&tree);
    assert!(open.get(), "Escape in the submenu closes only the submenu");
    assert_eq!(
        focus::current(),
        Some(menu),
        "and the keyboard is back on the menu"
    );
    route(&mut tree, &named(NamedKey::Escape));
    settle(&tree);
    assert!(!open.get(), "Escape on the menu asked to be closed");
    assert_eq!(
        focus::current(),
        Some(button),
        "and the keyboard is back where it was"
    );
}

/// A pick closes the menu as Escape does, and the keyboard comes back the same way, from as deep as a submenu.
#[test]
fn a_pick_gives_the_keyboard_back_too() {
    let mut lines = rows(1);
    lines.push(Line::Sub("más", rows(2)));
    let open = telar::signal(false);
    let (mut tree, button) = hosted(open, lines);
    open.set(true);
    settle(&tree);
    route(&mut tree, &named(NamedKey::End));
    route(&mut tree, &named(NamedKey::ArrowRight));
    settle(&tree);
    route(&mut tree, &named(NamedKey::Enter));
    settle(&tree);
    assert!(!open.get());
    assert_eq!(focus::current(), Some(button));
}

/// Asked for from the keyboard, a context menu opens under the control the keyboard is on, the menu's offset away, with the cursor on its first row.
#[test]
fn a_menu_asked_for_from_the_keyboard_opens_under_the_focused_control() {
    fresh_layout_runtime();
    focus::clear();
    let said: Said = Rc::default();
    let control = Rect::new(40.0, 50.0, 100.0, 30.0);
    let menu = context_menu(
        ContextMenuProps::props()
            .around(control)
            .within(WINDOW)
            .build(),
        written(rows(3), &said),
    )
    .unwrap();
    lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
    let mut tree = ComponentList::new(menu);
    settle(&tree);
    let panel = panels()[0];
    assert_eq!(
        panel.y - crate::shared::panel_pad(),
        bottom(control) + crate::shared::popup_offset()
    );
    route(&mut tree, &named(NamedKey::Enter));
    assert_eq!(*said.borrow(), vec!["fila 0"]);
}

#[test]
fn the_context_menu_key_and_shift_f10_ask_for_a_context_menu() {
    assert!(is_context_menu_key(&Key::Named(NamedKey::ContextMenu)));
    telar::testing::reset_keyboard();
    assert!(!is_context_menu_key(&Key::Named(NamedKey::F10)));
    assert!(!is_context_menu_key(&Key::Char('m')));
}

/// The rows, the tokens, the keyboard and the submenus of a menu are the panel's, not the opener's: a `menu` opened from its button and a `context_menu` opened at a point show the same panel.
#[test]
fn both_openers_open_the_same_panel() {
    let window = Rect::new(0.0, 0.0, 800.0, 600.0);
    let lines = || {
        vec![
            Line::Hinted("copiar", "ctrl+c"),
            Line::Rule,
            Line::Heading("más"),
            Line::Disabled("pegar"),
            Line::Sub("compartir", rows(2)),
        ]
    };
    let shape = |tree: &mut ComponentList, said: &Said| {
        settle(tree);
        let panel = panels()[0];
        let rows: Vec<(f32, f32)> = drawn(Role::MenuItem)
            .iter()
            .map(|row| (row.width, row.height))
            .collect();
        let paint = tree
            .commands()
            .iter()
            .find_map(|command| match command {
                telar::DrawCommand::Rect { style, .. } if style.border.is_some() => Some(**style),
                _ => None,
            })
            .expect("the panel is painted");
        let texts = drawn_texts(tree)
            .into_iter()
            .filter(|text| text != "Abrir")
            .collect::<Vec<_>>();
        route(tree, &named(NamedKey::End));
        route(tree, &named(NamedKey::ArrowRight));
        settle(tree);
        let sub = panels().get(1).copied().expect("the submenu opened");
        route(tree, &named(NamedKey::End));
        route(tree, &named(NamedKey::Enter));
        settle(tree);
        (
            panel.width,
            panel.height,
            sub.width,
            rows,
            paint,
            texts,
            said.borrow().clone(),
        )
    };

    fresh_layout_runtime();
    focus::clear();
    let said: Said = Rc::default();
    let mut tree = scoped(|| {
        let opened = context_menu(
            ContextMenuProps::props()
                .at((40.0, 30.0))
                .within(window)
                .build(),
            written(lines(), &said),
        )
        .unwrap();
        lay_out(opened.layout_node(), window.width, window.height);
        ComponentList::new(opened)
    });
    let at_a_point = shape(&mut tree, &said);
    drop(tree);

    fresh_layout_runtime();
    focus::clear();
    let said: Said = Rc::default();
    let mut tree = scoped(|| {
        let dropped = menu(
            MenuProps::props().label("Abrir").build(),
            written(lines(), &said),
        )
        .unwrap();
        lay_out(dropped.layout_node(), window.width, window.height);
        ComponentList::new(dropped)
    });
    settle(&tree);
    focus::focus_next();
    route(&mut tree, &named(NamedKey::Enter));
    let from_a_button = shape(&mut tree, &said);

    assert_eq!(at_a_point, from_a_button);
    assert_eq!(
        at_a_point.6,
        vec!["fila 1"],
        "a pick in the submenu, in both"
    );
}

/// A theme in one of two schemes, with every colour a menu could fall back to told apart.
#[derive(Clone)]
struct Scheme {
    dark: bool,
    menus: Option<MenuTokens>,
}

impl Scheme {
    fn pick(&self, light: Color, dark: Color) -> Color {
        if self.dark { dark } else { light }
    }
}

impl telar::ThemeTokens for Scheme {
    fn surface(&self) -> Color {
        self.pick(
            Color::rgba(0.98, 0.98, 0.97, 1.0),
            Color::rgba(0.11, 0.11, 0.13, 1.0),
        )
    }
    fn border(&self) -> Color {
        self.pick(
            Color::rgba(0.85, 0.85, 0.84, 1.0),
            Color::rgba(0.26, 0.26, 0.3, 1.0),
        )
    }
    fn ink(&self) -> Color {
        self.pick(
            Color::rgba(0.1, 0.1, 0.12, 1.0),
            Color::rgba(0.93, 0.93, 0.95, 1.0),
        )
    }
    fn muted(&self) -> Color {
        self.pick(
            Color::rgba(0.45, 0.45, 0.5, 1.0),
            Color::rgba(0.6, 0.6, 0.66, 1.0),
        )
    }
    fn radius(&self) -> f32 {
        4.0
    }
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(menus) = self.menus {
            extensions.insert(menus);
        }
    }
}

/// The paint of the first panel drawn: the first box with a frame.
fn panel_paint(tree: &ComponentList) -> RectStyle {
    tree.commands()
        .iter()
        .find_map(|command| match command {
            telar::DrawCommand::Rect { style, .. } if style.border.is_some() => Some(**style),
            _ => None,
        })
        .expect("the panel is painted")
}

/// The colour a string is drawn in.
fn ink_of(tree: &ComponentList, needle: &str) -> Color {
    tree.commands()
        .iter()
        .find_map(|command| match command {
            telar::DrawCommand::Text { text, style, .. } if text.as_ref() == needle => {
                Some(style.color.solid_color())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{needle:?} is drawn"))
}

/// Every colour comes from the theme in force, and a switch of scheme while the menu is open repaints it.
#[test]
fn the_panel_is_drawn_from_the_theme_in_force_in_light_and_in_dark() {
    use telar::{BorderRadius, Paint};

    let light = Scheme {
        dark: false,
        menus: None,
    };
    let dark = Scheme {
        dark: true,
        menus: None,
    };
    telar::set_theme(light.clone());
    let (tree, _) = opened();
    let panel = panel_paint(&tree);
    assert_eq!(panel.fill, Some(Paint::Solid(light.surface())));
    assert_eq!(
        panel.border.map(|border| border.paint),
        Some(Paint::Solid(light.border()))
    );
    assert_eq!(panel.radius, BorderRadius::all(light.radius_surface()));
    assert_eq!(ink_of(&tree, "copiar"), light.ink());
    assert_eq!(ink_of(&tree, "ctrl+c"), light.muted());
    assert_eq!(ink_of(&tree, "pegar"), light.muted(), "disabled");

    telar::set_theme(dark.clone());
    relayout_if_dirty();
    let panel = panel_paint(&tree);
    assert_eq!(panel.fill, Some(Paint::Solid(dark.surface())));
    assert_eq!(
        panel.border.map(|border| border.paint),
        Some(Paint::Solid(dark.border()))
    );
    assert_eq!(ink_of(&tree, "copiar"), dark.ink());
    assert_eq!(ink_of(&tree, "ctrl+c"), dark.muted());
}

/// The row under the keyboard wears the theme's hover wash in the corner every list's rows share.
#[test]
fn the_highlighted_row_wears_the_theme_s_hover_wash() {
    use telar::{BorderRadius, Paint};

    telar::set_theme(Scheme {
        dark: false,
        menus: None,
    });
    let (mut tree, _) = opened();
    route(&mut tree, &named(NamedKey::ArrowDown));
    relayout_if_dirty();
    let row = drawn(Role::MenuItem)[0];
    let lit = tree
        .commands()
        .iter()
        .find_map(|command| match command {
            telar::DrawCommand::Rect { rect, style }
                if rect.width == row.width && rect.height == row.height && style.fill.is_some() =>
            {
                Some(**style)
            }
            _ => None,
        })
        .expect("the highlighted row is painted");
    assert_eq!(lit.fill, Some(Paint::Solid(crate::shared::hover_wash())));
    assert_eq!(lit.radius, BorderRadius::all(crate::shared::radius_item()));
    assert_eq!(row.height, ListTokens::current().row_height());
}

/// The theme's menu tokens move every menu; one menu's own `tokens` win over them field by field; `style` amends the panel's paint last.
#[test]
fn a_menu_s_tokens_win_over_the_theme_s_and_its_style_amends_the_panel() {
    use telar::{BorderRadius, Paint};

    let red = Color::rgba(0.9, 0.1, 0.1, 1.0);
    let blue = Color::rgba(0.1, 0.1, 0.9, 1.0);
    telar::set_theme(Scheme {
        dark: false,
        menus: Some(MenuTokens {
            background: Some(red),
            radius: Some(3.0),
            row_height: Some(36.0),
            ..MenuTokens::default()
        }),
    });
    fresh_layout_runtime();
    let said: Said = Rc::default();
    let menu = context_menu(
        ContextMenuProps::props()
            .within(WINDOW)
            .tokens(MenuTokens {
                background: Some(blue),
                ..MenuTokens::default()
            })
            .style(Rc::new(|style: RectStyle| RectStyle {
                border: None,
                ..style
            }))
            .build(),
        written(vec![Line::Row("copiar")], &said),
    )
    .unwrap();
    lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
    let tree = ComponentList::new(menu);
    let panel = tree
        .commands()
        .iter()
        .find_map(|command| match command {
            telar::DrawCommand::Rect { style, .. } if style.fill.is_some() => Some(**style),
            _ => None,
        })
        .expect("the panel is painted");
    assert_eq!(
        panel.fill,
        Some(Paint::Solid(blue)),
        "the menu's own colour"
    );
    assert_eq!(panel.radius, BorderRadius::all(3.0), "the theme's radius");
    assert_eq!(panel.border, None, "and the caller's amendment last");
    assert_eq!(drawn(Role::MenuItem)[0].height, 36.0);
}

/// A panel is as wide as its widest row, so a long label and the hint beside it are both said in full, and never narrower than its tokens' width.
#[test]
fn a_panel_widens_to_its_widest_row() {
    fresh_layout_runtime();
    focus::clear();
    let said: Said = Rc::default();
    let tree = scoped(|| {
        let menu = context_menu(
            ContextMenuProps::props().within(WINDOW).build(),
            written(
                vec![Line::Hinted(
                    "a rather long label here",
                    "and a long hint too",
                )],
                &said,
            ),
        )
        .unwrap();
        lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
        ComponentList::new(menu)
    });
    settle(&tree);
    let width_of = |needle: &str| {
        tree.commands()
            .iter()
            .find_map(|command| match command {
                telar::DrawCommand::Text { text, rect, .. } if text.as_ref() == needle => {
                    Some(rect.width)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("{needle:?} is drawn"))
    };
    let label = width_of("a rather long label here");
    let measured = telar::measure_text(
        "a rather long label here",
        None,
        f32::INFINITY,
        &telar::Inherited::initial().text,
    )
    .0;
    assert!(
        label >= measured - 0.5,
        "the label is whole: {label} of {measured}"
    );
    assert!(panels()[0].width > MenuTokens::current().width());
    drop(tree);

    let (_tree, _) = opened_at((40.0, 30.0), rows(2));
    assert_eq!(
        panels()[0].width,
        120.0 - crate::shared::panel_pad() * 2.0,
        "short rows keep the least width"
    );
}

/// "Modo" (plain) · "Reducir" (a switch bound to the returned signal, flipped by its own `on_press`) · "Reglas" (ticked), as a menu with the given number of ticked rows.
fn switching(ticked: bool) -> (Scoped<ComponentList>, RwSignal<bool>, Said) {
    fresh_layout_runtime();
    focus::clear();
    let said: Said = Rc::default();
    let closing = said.clone();
    let on = signal(false);
    let tree = scoped(|| {
        let rows = Children::new(move || {
            let mut slots = Slots::new();
            slots.push(
                None,
                item(
                    ItemProps::props().label("Modo").build(),
                    Children::default(),
                )?,
            );
            slots.push(
                None,
                item(
                    ItemProps::props()
                        .label("Reducir")
                        .switch(Reactive::of(move || on.get()))
                        .on_press(Rc::new(move || on.update(|on| *on = !*on)) as Rc<dyn Fn()>)
                        .build(),
                    Children::default(),
                )?,
            );
            if ticked {
                slots.push(
                    None,
                    item(
                        ItemProps::props().label("Reglas").checked(true).build(),
                        Children::default(),
                    )?,
                );
            }
            Ok(slots)
        });
        let menu = context_menu(
            ContextMenuProps::props()
                .at((40.0, 30.0))
                .on_close(Rc::new(move || closing.borrow_mut().push("cerrar".into())))
                .width(160.0)
                .within(WINDOW)
                .build(),
            rows,
        )
        .unwrap();
        lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
        ComponentList::new(menu)
    });
    settle(&tree);
    (tree, on, said)
}

fn text_rect(tree: &ComponentList, needle: &str) -> Rect {
    let mut found = None;
    telar::for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let telar::DrawCommand::Text { text, rect, .. } = command
            && text.as_ref() == needle
        {
            found.get_or_insert(telar::transform_clip_rect(matrix, *rect));
        }
    });
    found.unwrap_or_else(|| panic!("{needle:?} is drawn"))
}

/// The switch's track: the one box drawn at the size a row's switch is.
fn track_rect(tree: &ComponentList) -> Rect {
    let size = crate::toggle::SwitchSize::row();
    let (width, height) = (
        size.knob * 2.0 + size.inset * 2.0,
        size.knob + size.inset * 2.0,
    );
    let mut found = None;
    telar::for_each_with_matrix(&tree.commands(), |command, matrix| {
        if let telar::DrawCommand::Rect { rect, .. } = command
            && (rect.width - width).abs() < 0.01
            && (rect.height - height).abs() < 0.01
        {
            found.get_or_insert(telar::transform_clip_rect(matrix, *rect));
        }
    });
    found.expect("the switch is drawn")
}

fn switch_row(tree: &ComponentList) -> Rect {
    let _ = tree;
    exposed_named_by(Role::MenuItemCheckBox)
}

fn exposed_named_by(role: Role) -> Rect {
    let exposed = focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == role)
        .expect("the row is exposed");
    telar::visible_rect(exposed.node).expect("the row is laid out")
}

/// A switch stands where a tick would not: the labels of a panel with one keep the x they have without.
#[test]
fn a_switch_row_leaves_the_tick_column_unclaimed() {
    let (tree, _, _) = switching(false);
    let flush = text_rect(&tree, "Modo").x;
    assert_eq!(flush, text_rect(&tree, "Reducir").x, "the labels are flush");
    drop(tree);
    let (ticked, _, _) = switching(true);
    assert!(
        text_rect(&ticked, "Modo").x > flush,
        "a tick still reserves its column: {} vs {}",
        text_rect(&ticked, "Modo").x,
        flush
    );
}

/// The switch sits at the end of the row, inside it, and is not a control of its own.
#[test]
fn a_switch_row_draws_its_switch_at_the_end_and_names_it_once() {
    let (tree, _, _) = switching(false);
    let row = switch_row(&tree);
    let track = track_rect(&tree);
    assert!(track.x + track.width <= row.x + row.width);
    assert!(
        track.x > text_rect(&tree, "Reducir").x,
        "the switch follows the label"
    );
    let checkboxes: Vec<_> = focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::MenuItemCheckBox)
        .collect();
    assert_eq!(checkboxes.len(), 1);
    assert_eq!(checkboxes[0].toggled, Some(false));
    assert!(
        focus::exposed()
            .iter()
            .all(|exposed| exposed.role != Role::Switch),
        "the drawn switch is hidden from readers"
    );
}

/// Click, Enter and Space each flip the setting, and none of them shuts the panel: it is a setting, not an action.
#[test]
fn a_switch_row_toggles_without_closing_the_panel() {
    let (mut tree, on, said) = switching(false);
    let (x, y) = centre(switch_row(&tree));
    route(&mut tree, &press(x, y));
    route(&mut tree, &release(x, y));
    settle(&tree);
    assert!(on.get(), "a click switches it on");
    route(&mut tree, &moved(x, y));
    settle(&tree);

    route(&mut tree, &named(NamedKey::Enter));
    settle(&tree);
    assert!(!on.get(), "Enter switches it off");

    route(&mut tree, &named(NamedKey::Space));
    settle(&tree);
    assert!(on.get(), "Space switches it on");

    assert!(
        said.borrow().is_empty(),
        "the panel was never asked to close: {:?}",
        said.borrow()
    );
    assert_eq!(panels().len(), 1);
    let checkbox = focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == Role::MenuItemCheckBox)
        .expect("the row is exposed");
    assert_eq!(checkbox.toggled, Some(true));

    route(&mut tree, &named(NamedKey::Escape));
    assert_eq!(*said.borrow(), vec!["cerrar"], "Escape still closes it");
}

/// The track and knob follow the control size in force, as a standalone switch does.
#[test]
fn a_switch_row_scales_with_the_control_size() {
    let (tree, _, _) = switching(false);
    let regular = track_rect(&tree);
    drop(tree);
    telar::set_control_size(telar::ControlSize::Large);
    let (tree, _, _) = switching(false);
    let large = track_rect(&tree);
    telar::set_control_size(telar::ControlSize::Regular);
    let scale = telar::ControlSize::Large.scale();
    assert_eq!(
        (large.width, large.height),
        (regular.width * scale, regular.height * scale)
    );
}

/// In a right-to-left region the switch is at the row's left edge, its logical end.
#[test]
fn a_switch_row_puts_its_switch_at_the_logical_end_in_a_right_to_left_region() {
    telar::set_direction(telar::Direction::Rtl);
    let (tree, _, _) = switching(false);
    let row = switch_row(&tree);
    let track = track_rect(&tree);
    assert!(track.x < row.x + row.width / 2.0, "{track:?} in {row:?}");
    assert!(
        text_rect(&tree, "Reducir").x >= track.x + track.width,
        "the label is on the switch's right"
    );
    telar::set_direction(telar::Direction::Ltr);
}
