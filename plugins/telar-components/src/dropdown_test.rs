use telar::testing::{Scoped, centre, named, press, release, route};
use telar::{AvailableSpace, ComponentList, NamedKey, compute_layout, relayout_if_dirty, signal};

use super::*;
use crate::select::{SelectProps, select};
use crate::shared::{panel_pad, row_height};
use crate::test_support::{drawn, fresh_layout_runtime};

const WINDOW_WIDTH: f32 = 400.0;
const WINDOW_HEIGHT: f32 = 300.0;

fn rows(count: u32) -> Children {
    Children::new(move || {
        let mut items = Vec::new();
        for n in 0..count {
            items.push(crate::list::item(
                crate::list::ItemProps::props()
                    .label(Reactive::of(move || format!("Row {n}")))
                    .build(),
            )?);
        }
        Ok(items)
    })
}

fn settle(tree: &ComponentList) {
    for _ in 0..3 {
        relayout_if_dirty();
        let _ = tree.commands();
    }
}

/// A select of `count` rows whose trigger stands `top` px down the window, opened from the keyboard.
fn opened(count: u32, top: f32) -> (Scoped<ComponentList>, RwSignal<u32>) {
    fresh_layout_runtime();
    let picked = signal(0u32);
    let mut tree = telar::testing::scoped(|| {
        let field = select(
            SelectProps::props()
                .selected(picked)
                .children(rows(count))
                .build(),
        )
        .unwrap();
        let root = Container::new(
            LayoutStyle::new().flex_column().padding_top(top),
            vec![field],
        )
        .unwrap();
        compute_layout(
            root.layout_node(),
            AvailableSpace::Definite(WINDOW_WIDTH),
            AvailableSpace::Definite(WINDOW_HEIGHT),
        )
        .unwrap();
        ComponentList::new(root)
    });
    settle(&tree);
    telar::focus::focus_next();
    route(&mut tree, &named(NamedKey::Enter));
    settle(&tree);
    (tree, picked)
}

fn list() -> Rect {
    let lists = drawn(Role::ListBox);
    assert_eq!(lists.len(), 1, "one open list: {lists:?}");
    lists[0]
}

fn bottom(rect: Rect) -> f32 {
    rect.y + rect.height
}

#[test]
fn a_short_list_is_as_tall_as_its_rows() {
    let (_tree, _) = opened(3, 0.0);
    let list = list();
    assert_eq!(list.height, 3.0 * row_height());
    assert_eq!(
        list.y,
        crate::shared::control_height() + crate::shared::popup_offset() + panel_pad(),
        "under its trigger"
    );
}

/// Forty rows are 1280px; in a 300px window the list used to open at its full height and run off the bottom of the surface.
#[test]
fn a_long_list_is_capped_to_the_window_and_scrolls_to_the_cursor() {
    let (mut tree, picked) = opened(40, 0.0);
    let list = list();
    assert!(
        bottom(list) + panel_pad() <= WINDOW_HEIGHT,
        "inside the window: {list:?}"
    );
    assert!(
        list.height < 40.0 * row_height(),
        "cut to the room: {list:?}"
    );

    route(&mut tree, &named(NamedKey::End));
    settle(&tree);
    let last = *drawn(Role::Option).last().expect("the rows are announced");
    assert!(
        last.y >= list.y - 0.5 && bottom(last) <= bottom(list) + 0.5,
        "the cursor's row is scrolled into the list: {last:?} in {list:?}"
    );

    let (x, y) = centre(last);
    route(&mut tree, &press(x, y));
    route(&mut tree, &release(x, y));
    assert_eq!(
        picked.get(),
        39,
        "and the row drawn there is the one pressed"
    );
}

#[test]
fn a_dropdown_near_the_bottom_opens_above_its_trigger() {
    let top = 220.0;
    let (mut tree, picked) = opened(5, top);
    let list = list();
    assert_eq!(
        list.height,
        5.0 * row_height(),
        "whole, since it fits above"
    );
    assert!(
        bottom(list) + panel_pad() <= top,
        "above the trigger: {list:?}"
    );

    let first = drawn(Role::Option)[0];
    let (x, y) = centre(first);
    route(&mut tree, &press(x, y));
    route(&mut tree, &release(x, y));
    assert_eq!(picked.get(), 0, "a flipped list answers where it is drawn");
}

#[test]
fn a_long_list_near_the_bottom_is_capped_to_the_room_above() {
    let top = 220.0;
    let (_tree, _) = opened(40, top);
    let list = list();
    assert!(
        bottom(list) + panel_pad() <= top,
        "above the trigger: {list:?}"
    );
    assert!(list.y - panel_pad() >= 0.0, "inside the window: {list:?}");
}

#[test]
fn a_dropdown_with_more_room_below_stays_below_however_long_it_is() {
    let top = 100.0;
    let (_tree, _) = opened(40, top);
    let list = list();
    assert!(
        list.y >= top + crate::shared::control_height(),
        "below the trigger: {list:?}"
    );
    assert!(bottom(list) <= WINDOW_HEIGHT, "inside the window: {list:?}");
}

fn exposed(role: Role) -> telar::focus::Exposed {
    telar::focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == role)
        .unwrap_or_else(|| panic!("a {role:?} is exposed"))
}

/// A select is a combobox whose list is only ever pointed into: the keyboard stays on the trigger, which names the list it opened and the row its cursor is on, and Tab finds no stop in the list.
#[test]
fn an_open_select_keeps_the_keyboard_and_points_into_its_list() {
    let (_tree, _) = opened(3, 0.0);
    let trigger = exposed(Role::ComboBox).id;
    let list = exposed(Role::ListBox).id;
    assert_eq!(telar::focus::current(), Some(trigger));
    assert_eq!(telar::focus::controls_state(trigger), Some(list));
    let first_row = telar::focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == Role::Option)
        .expect("the rows are announced")
        .id;
    assert_eq!(
        telar::focus::active_descendant_state(trigger),
        Some(first_row),
        "the cursor starts on what is chosen"
    );
    assert!(
        telar::focus::tab_order().iter().all(|stop| stop.id != list),
        "the list is no Tab stop"
    );
    telar::focus::focus_next();
    assert_eq!(telar::focus::current(), Some(trigger), "Tab stays put");
}

/// Whichever way an open select is closed, the keyboard is on it afterwards: Escape and a pick leave it there, and a press away — which the runner answers by clearing focus before the press arrives — gives it back.
#[test]
fn every_way_of_closing_a_select_leaves_the_keyboard_on_it() {
    let (mut tree, picked) = opened(3, 0.0);
    let trigger = exposed(Role::ComboBox).id;
    route(&mut tree, &named(NamedKey::Escape));
    settle(&tree);
    assert_eq!(telar::focus::current(), Some(trigger), "after Escape");
    assert_eq!(telar::focus::controls_state(trigger), None, "shut");

    route(&mut tree, &named(NamedKey::Enter));
    settle(&tree);
    let (x, y) = centre(drawn(Role::Option)[2]);
    telar::focus::blur_from_pointer(x as f32, y as f32);
    route(&mut tree, &press(x, y));
    route(&mut tree, &release(x, y));
    settle(&tree);
    assert_eq!(picked.get(), 2);
    assert_eq!(telar::focus::current(), Some(trigger), "after a pick");

    route(&mut tree, &named(NamedKey::Enter));
    settle(&tree);
    let (x, y) = (WINDOW_WIDTH as f64 - 10.0, WINDOW_HEIGHT as f64 - 10.0);
    telar::focus::blur_from_pointer(x as f32, y as f32);
    assert_eq!(telar::focus::current(), None, "a press away clears focus");
    route(&mut tree, &press(x, y));
    route(&mut tree, &release(x, y));
    settle(&tree);
    assert_eq!(
        telar::focus::current(),
        Some(trigger),
        "and closing gives it back"
    );
    assert!(
        telar::focus::is_focus_visible(trigger),
        "as the keyboard had it, ring and all"
    );
}

/// A select's list is a list box of options, each saying whether it is the bound choice: the one picked, not the one the cursor is on.
#[test]
fn a_select_s_rows_are_options_that_say_which_one_is_chosen() {
    let (mut tree, picked) = opened(3, 0.0);
    let selection = || -> Vec<Option<bool>> {
        telar::focus::exposed()
            .into_iter()
            .filter(|exposed| exposed.role == Role::Option)
            .map(|exposed| exposed.toggled)
            .collect()
    };
    assert_eq!(selection(), [Some(true), Some(false), Some(false)]);
    assert!(
        telar::focus::exposed()
            .iter()
            .all(|exposed| exposed.role != Role::MenuItem && exposed.role != Role::Menu),
        "nothing in a select is announced as a menu"
    );

    route(&mut tree, &named(NamedKey::ArrowDown));
    settle(&tree);
    assert_eq!(
        selection(),
        [Some(true), Some(false), Some(false)],
        "moving the cursor chooses nothing"
    );

    route(&mut tree, &named(NamedKey::Enter));
    settle(&tree);
    assert_eq!(picked.get(), 1);
    route(&mut tree, &named(NamedKey::Enter));
    settle(&tree);
    assert_eq!(selection(), [Some(false), Some(true), Some(false)]);
}

#[test]
#[cfg(all(feature = "workbench", feature = "overlays"))]
fn controls_of_every_kind_standing_in_a_row_are_drawn_at_one_height() {
    use crate::{ButtonProps, IconButtonProps, MenuProps, button, icon_button, menu};
    use telar::focus::Role;

    fresh_layout_runtime();
    let items = vec![
        button(ButtonProps::props().label("Open").build()).unwrap(),
        icon_button(
            IconButtonProps::props()
                .icon("lucide:search")
                .label("Find")
                .build(),
        )
        .unwrap(),
        menu(MenuProps::props().label("File").fit(true).build()).unwrap(),
        select(SelectProps::props().build()).unwrap(),
    ];
    let root = telar::Container::new(
        telar::LayoutStyle::new()
            .flex_row()
            .align_items(telar::AlignItems::CENTER)
            .width(WINDOW_WIDTH)
            .height(WINDOW_HEIGHT),
        items,
    )
    .unwrap();
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(WINDOW_WIDTH),
        AvailableSpace::Definite(WINDOW_HEIGHT),
    )
    .unwrap();
    relayout_if_dirty();
    let _tree = ComponentList::new(telar::box_item(root));
    let heights: Vec<f32> = [Role::Button, Role::ComboBox]
        .into_iter()
        .flat_map(drawn)
        .map(|rect| rect.height)
        .collect();
    assert_eq!(heights.len(), 4, "{heights:?}");
    assert!(
        heights
            .iter()
            .all(|height| *height == crate::shared::control_height()),
        "{heights:?}"
    );
}

/// A select's list keeps the popup offset from its field on whichever side it opens, and its tokens name one of their own.
#[test]
fn a_list_keeps_the_popup_offset_from_its_field_on_either_side() {
    let offset = crate::shared::popup_offset();
    let (flipped, _) = opened(5, 220.0);
    assert_eq!(
        bottom(list()) + panel_pad(),
        220.0 - offset,
        "over the field, flipped"
    );
    drop(flipped);

    fresh_layout_runtime();
    telar::set_theme(crate::test_support::TokensTheme {
        role: 4.0,
        tokens: Some(crate::SelectTokens {
            offset: Some(20.0),
            ..crate::SelectTokens::default()
        }),
    });
    let picked = signal(0u32);
    let field = select(
        SelectProps::props()
            .selected(picked)
            .children(rows(3))
            .build(),
    )
    .unwrap();
    compute_layout(
        field.layout_node(),
        AvailableSpace::Definite(WINDOW_WIDTH),
        AvailableSpace::Definite(WINDOW_HEIGHT),
    )
    .unwrap();
    let mut tree = ComponentList::new(field);
    settle(&tree);
    telar::focus::focus_next();
    route(&mut tree, &named(NamedKey::Enter));
    settle(&tree);
    let shown = list();
    assert_eq!(
        shown.y - panel_pad(),
        crate::shared::control_height() + 20.0
    );
    telar::set_theme(crate::test_support::TokensTheme::<crate::SelectTokens> {
        role: 4.0,
        tokens: None,
    });
}
