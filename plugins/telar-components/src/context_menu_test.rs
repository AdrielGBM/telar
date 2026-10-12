use std::cell::RefCell;

use telar::testing::{lay_out, moved, named, press, release, route};
use telar::{ComponentList, LayoutItem, ThemeTokens, relayout_if_dirty};

use super::*;
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
            telar::DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
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
        let mut slots = telar::Slots::new();
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

    route(&mut tree, &named(NamedKey::ArrowDown));
    route(&mut tree, &named(NamedKey::ArrowDown));
    route(&mut tree, &named(NamedKey::Enter));
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

/// Rightwards into a submenu, leftwards out of it, and what is picked inside it is picked.
#[test]
fn a_submenu_opens_beside_its_row_and_answers_the_keyboard() {
    let (mut tree, said) = menu();
    route(&mut tree, &named(NamedKey::End));
    route(&mut tree, &named(NamedKey::ArrowRight));
    telar::relayout_if_dirty();

    assert!(
        drawn_texts(&tree).iter().any(|text| text == "hondo"),
        "el submenú no se abrió: {:?}",
        drawn_texts(&tree)
    );

    route(&mut tree, &named(NamedKey::ArrowDown));
    route(&mut tree, &named(NamedKey::Enter));
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

    let (x, y) = telar::testing::centre(crate::test_support::drawn(Role::MenuItem)[2]);
    route(&mut tree, &moved(x, y));
    telar::relayout_if_dirty();
    assert!(
        drawn_texts(&tree).iter().any(|text| text == "hondo"),
        "el submenú no se abrió al apuntarlo: {:?}",
        drawn_texts(&tree)
    );

    route(&mut tree, &moved(60.0, 45.0));
    telar::relayout_if_dirty();
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
    route(&mut tree, &named(NamedKey::Escape));
    assert_eq!(*said.borrow(), vec!["cerrar"], "escape no lo cerró");
}

#[test]
fn declaring_the_rows_leaves_no_layout_nodes_behind() {
    fresh_layout_runtime();
    let labels = telar::signal(vec!["b", "c"]);
    let rows = Children::new(move || {
        let row = |label: &'static str| {
            menu_row(
                MenuRowProps::props().label(label).build(),
                Children::default(),
            )
        };
        let mut slots = telar::Slots::new();
        slots.push(
            None,
            box_item(telar::Container::new(LayoutStyle::new(), vec![row("a")?])?),
        );
        slots.push(
            None,
            box_item(telar::ReactiveList::new(
                move || labels.get(),
                |label: &&str| *label,
                row,
                0.0,
            )?),
        );
        Ok(slots)
    });
    let baseline = ui_core::live_node_count();

    let entries = declared(&rows).unwrap();

    assert_eq!(entries.len(), 3);
    assert_eq!(ui_core::live_node_count(), baseline);
}

fn rows(count: usize) -> Vec<Entry> {
    (0..count)
        .map(|n| Entry::row(format!("fila {n}"), "", || {}))
        .collect()
}

/// A menu of `entries` asked for at `at` in [`WINDOW`], laid out and drawn.
fn opened_at(at: (f32, f32), entries: Vec<Entry>) -> ComponentList {
    fresh_layout_runtime();
    let menu = context_menu(
        ContextMenuProps::props()
            .at(at)
            .entries(entries)
            .width(120.0)
            .within(WINDOW)
            .build(),
        Children::default(),
    )
    .unwrap();
    lay_out(menu.layout_node(), WINDOW.width, WINDOW.height);
    let tree = ComponentList::new(menu);
    settle(&tree);
    tree
}

fn settle(tree: &ComponentList) {
    for _ in 0..3 {
        telar::relayout_if_dirty();
        let _ = tree.commands();
    }
}

fn panels() -> Vec<Rect> {
    crate::test_support::drawn(Role::Menu)
}

fn bottom(rect: Rect) -> f32 {
    rect.y + rect.height
}

/// Thirty rows are taller than the window; the panel used to be pushed up to its top edge and still run off the bottom.
#[test]
fn a_long_menu_is_capped_to_the_window_and_scrolls_to_the_keyboard() {
    let mut tree = opened_at((40.0, 30.0), rows(30));
    let panel = panels()[0];
    assert_eq!(panel.y, 30.0 + crate::shared::panel_pad(), "at the point");
    assert!(
        bottom(panel) <= WINDOW.height,
        "inside the window: {panel:?}"
    );

    route(&mut tree, &named(NamedKey::End));
    settle(&tree);
    let last = *crate::test_support::drawn(Role::MenuItem)
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
    let tree = opened_at(point, rows(3));
    let panel = panels()[0];
    assert!(
        bottom(panel) <= point.1,
        "above the point it was asked at: {panel:?}"
    );
    assert!(
        drawn_texts(&tree).contains(&"fila 0".to_string()),
        "and whole"
    );
}

/// Beside the right edge a submenu opens on the left of its row, and one low in the window is lifted to stay in it.
#[test]
fn a_submenu_opens_on_the_side_with_room_and_stays_in_the_window() {
    let mut entries = rows(4);
    entries.push(Entry::Sub {
        label: "más".into(),
        entries: rows(6),
    });
    let mut tree = opened_at((300.0, 120.0), entries);
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

/// A button that had the keyboard, and a menu shown while `open` holds, the way an application hosts one: closing it is the caller taking it out of the tree.
fn hosted(open: RwSignal<bool>, entries: Vec<Entry>) -> (ComponentList, FocusId) {
    fresh_layout_runtime();
    focus::clear();
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
                    .entries(entries.clone())
                    .on_close(Rc::new(move || open.set(false)))
                    .width(120.0)
                    .within(WINDOW)
                    .build(),
                Children::default(),
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

fn lists() -> Vec<FocusId> {
    focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::Menu)
        .map(|exposed| exposed.id)
        .collect()
}

/// A context menu takes the keyboard as it opens, hands it to a submenu as the keys go there and back as they return, and gives it back to whatever held it when the menu goes.
#[test]
fn the_keyboard_goes_into_the_menu_follows_its_submenus_and_comes_back() {
    let mut entries = rows(2);
    entries.push(Entry::Sub {
        label: "más".into(),
        entries: rows(2),
    });
    let open = telar::signal(false);
    let (mut tree, button) = hosted(open, entries);

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
    assert!(!open.get(), "Escape asked to be closed");
    assert_eq!(
        focus::current(),
        Some(button),
        "and the keyboard is back where it was, from as deep as a submenu"
    );
}

/// A pick closes the menu as Escape does, and the keyboard comes back the same way.
#[test]
fn a_pick_gives_the_keyboard_back_too() {
    let open = telar::signal(false);
    let (mut tree, button) = hosted(open, rows(3));
    open.set(true);
    settle(&tree);
    route(&mut tree, &named(NamedKey::ArrowDown));
    route(&mut tree, &named(NamedKey::Enter));
    settle(&tree);
    assert!(!open.get());
    assert_eq!(focus::current(), Some(button));
}

/// A theme in one of two schemes, with every colour a context menu could fall back to told apart.
#[derive(Clone)]
struct Scheme {
    dark: bool,
    menus: Option<ContextMenuTokens>,
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

/// The panel used to be a fixed near-black whatever the application looked like, so a light application opened a dark menu. Every colour now comes from the theme in force, and a switch of scheme while the menu is open repaints it.
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
    let (tree, _) = menu();
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

/// The row under the keyboard wears the theme's hover wash in the corner every list's rows share, as the rows of a `menu` do.
#[test]
fn the_highlighted_row_wears_the_theme_s_hover_wash() {
    use telar::{BorderRadius, Paint};

    telar::set_theme(Scheme {
        dark: false,
        menus: None,
    });
    let (mut tree, _) = menu();
    route(&mut tree, &named(NamedKey::ArrowDown));
    relayout_if_dirty();
    let row = crate::test_support::drawn(Role::MenuItem)[0];
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

/// The theme's context menu tokens move every menu; one menu's own `tokens` win over them field by field; `style` amends the panel's paint last.
#[test]
fn a_menu_s_tokens_win_over_the_theme_s_and_its_style_amends_the_panel() {
    use telar::{BorderRadius, Paint};

    let red = Color::rgba(0.9, 0.1, 0.1, 1.0);
    let blue = Color::rgba(0.1, 0.1, 0.9, 1.0);
    telar::set_theme(Scheme {
        dark: false,
        menus: Some(ContextMenuTokens {
            background: Some(red),
            radius: Some(3.0),
            row_height: Some(36.0),
            ..ContextMenuTokens::default()
        }),
    });
    fresh_layout_runtime();
    let menu = context_menu(
        ContextMenuProps::props()
            .entries(vec![Entry::row("copiar", "", || {})])
            .within(WINDOW)
            .tokens(ContextMenuTokens {
                background: Some(blue),
                ..ContextMenuTokens::default()
            })
            .style(Rc::new(|style: RectStyle| RectStyle {
                border: None,
                ..style
            }))
            .build(),
        Children::default(),
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
    assert_eq!(crate::test_support::drawn(Role::MenuItem)[0].height, 36.0);
}
