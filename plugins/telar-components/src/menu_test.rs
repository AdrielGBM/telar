use std::cell::Cell;
use std::rc::Rc;

use telar::testing::{named, press, release, route};
use telar::{
    AvailableSpace, ComponentList, Event, LayoutStyle, compute_layout, dispatch_overlays,
    relayout_if_dirty,
};

use super::*;

// Mirrors the runner: the overlay registry first, then the tree only if no overlay consumed the event.

/// A compact trigger does not squeeze the panel. `stretch` means "be at least as wide as the control I sit under", and taking that width outright turned a `File` button into a 40px sheet with one character per line — every item wrapped down its own column.
#[test]
fn a_narrow_filled_trigger_still_opens_a_readable_panel() {
    crate::test_support::fresh_layout_runtime();
    let item = menu(
        MenuProps::props().label("File").stretch(true).build(),
        rows(&["New", "Open…", "Save", "Import STEP…"]),
    )
    .unwrap();
    let root_node = item.layout_node();
    let row = telar::new_container(
        LayoutStyle::new().flex_row().width(44.0).height(400.0),
        &[root_node],
    )
    .unwrap();
    let window = telar::new_container(
        LayoutStyle::new().flex_column().width(400.0).height(400.0),
        &[row],
    )
    .unwrap();
    let mut tree = ComponentList::new(item);
    compute_layout(
        window,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let _ = tree.commands();

    route(&mut tree, &press(20.0, 18.0));
    route(&mut tree, &release(20.0, 18.0));
    relayout_if_dirty();
    let _ = tree.commands();

    let widths: Vec<f32> = tree
        .commands()
        .iter()
        .filter_map(|c| match c {
            telar::DrawCommand::Rect { rect, .. } => Some(rect.width),
            _ => None,
        })
        .collect();
    assert!(
        widths
            .iter()
            .any(|w| *w >= panel_width() - panel_pad() * 2.0 - 0.5),
        "the open panel should be at least {}px wide, got {widths:?}",
        panel_width()
    );
}

#[test]
fn builds_and_lays_out() {
    crate::test_support::fresh_layout_runtime();
    let item = menu(
        MenuProps::props().label("Actions").build(),
        rows(&["Rename", "Duplicate", "Delete"]),
    )
    .unwrap();
    let root_node = item.layout_node();
    let root_rect = track_layout(root_node).unwrap();
    compute_layout(
        root_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    assert!(
        root_rect.get().height >= crate::shared::control_height() - 0.5,
        "closed menu is at least the trigger tall: {:?}",
        root_rect.get()
    );
    let tree = ComponentList::new(item);
    let _ = tree.commands();
}

/// A menu was reachable by mouse and by nothing else: the trigger took no focus, so Tab passed it by, and the panel answered to no key at all. Radix gives arrows, Home/End and Escape away for free; here every one of them was absent, which is the difference between a control a keyboard user can operate and one they cannot.
#[test]
fn a_menu_can_be_opened_and_driven_from_the_keyboard() {
    use telar::NamedKey;

    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let item = menu(
        MenuProps::props()
            .label("Actions")
            .on_select(Rc::new(move |i| sink.set(Some(i))))
            .build(),
        rows(&["Rename", "Duplicate", "Delete"]),
    )
    .unwrap();
    let root_node = item.layout_node();
    compute_layout(
        root_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(item);

    telar::focus::focus_next();
    assert!(
        telar::focus::current().is_some(),
        "the trigger joined the tab order"
    );

    route(&mut tree, &named(NamedKey::ArrowDown));
    relayout_if_dirty();
    route(&mut tree, &named(NamedKey::ArrowDown));
    route(&mut tree, &named(NamedKey::Enter));
    assert_eq!(seen.get(), Some(1), "the highlighted row is what commits");
}

/// Escape closes it even though the trigger holds focus: only a focused field gets Escape before the dismiss stack, and a trigger is the control the menu belongs to.
#[test]
fn escape_closes_a_menu_whose_trigger_holds_focus() {
    use telar::NamedKey;

    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let item = menu(
        MenuProps::props()
            .label("Actions")
            .on_select(Rc::new(move |i| sink.set(Some(i))))
            .build(),
        rows(&["Rename", "Duplicate"]),
    )
    .unwrap();
    let root_node = item.layout_node();
    compute_layout(
        root_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(item);

    telar::focus::focus_next();
    route(&mut tree, &named(NamedKey::ArrowDown));
    relayout_if_dirty();
    route(&mut tree, &named(NamedKey::Escape));
    relayout_if_dirty();

    route(&mut tree, &named(NamedKey::Enter));
    assert_eq!(seen.get(), None, "Escape shut it before Enter could pick");
}

#[test]
fn selecting_an_item_fires_on_select_and_closes() {
    crate::test_support::fresh_layout_runtime();
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let item = menu(
        MenuProps::props()
            .label("Actions")
            .on_select(Rc::new(move |i| sink.set(Some(i))))
            .build(),
        rows(&["Rename", "Duplicate", "Delete"]),
    )
    .unwrap();
    // The widget's own root is the parent-less layout host laid out at the origin, so the trigger sits at (0,0) and the panel anchors directly below it.
    let root_node = item.layout_node();
    compute_layout(
        root_node,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(item);
    let _ = tree.commands();

    let tx = (panel_width() / 2.0) as f64;
    let ty = (crate::shared::control_height() / 2.0) as f64;
    route(&mut tree, &press(tx, ty));
    route(&mut tree, &release(tx, ty));
    relayout_if_dirty();

    let ox = (panel_width() / 2.0) as f64;
    let oy = (crate::shared::control_height()
        + crate::shared::popup_offset()
        + panel_pad()
        + row_height()
        + row_height() / 2.0) as f64;
    route(&mut tree, &press(ox, oy));
    route(&mut tree, &release(ox, oy));

    assert_eq!(
        seen.get(),
        Some(1),
        "picking the second item fires on_select(1)"
    );
    assert!(
        !dispatch_overlays(&press(ox, oy)),
        "the menu closes after a pick"
    );
}
/// What a menu could not say before, said end to end: a disabled row, a separator, and a heading are all *in* the list and none of them is a place the keyboard stops.
///
/// The three used to be inexpressible for the same reason — the rows were `Vec<&str>`, and a string carries no state — and they are testable together now for the same reason: each row builds itself inside the menu, so it can register what it is rather than being told.
#[test]
fn the_keyboard_steps_over_what_it_cannot_commit() {
    use telar::NamedKey;

    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let structured = Children::new(|| {
        let mut slots = Slots::new();
        let row = |label: &'static str, disabled: bool| {
            crate::list::item(
                crate::list::ItemProps::props()
                    .label(Reactive::of(move || label.to_string()))
                    .disabled(Reactive::of(move || disabled))
                    .build(),
                Children::default(),
            )
        };
        slots.push(None, row("Rename", false)?);
        slots.push(None, row("Duplicate", true)?);
        slots.push(
            None,
            crate::list::separator(
                crate::list::SeparatorProps::props().build(),
                Children::default(),
            )?,
        );
        slots.push(
            None,
            crate::list::group(
                crate::list::GroupProps::props().label("Danger").build(),
                Children::default(),
            )?,
        );
        slots.push(None, row("Delete", false)?);
        Ok(slots)
    });
    let item = menu(
        MenuProps::props()
            .label("Actions")
            .on_select(Rc::new(move |i| sink.set(Some(i))))
            .build(),
        structured,
    )
    .unwrap();
    compute_layout(
        item.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(item);

    telar::focus::focus_next();
    route(&mut tree, &named(NamedKey::ArrowDown));
    relayout_if_dirty();
    // One step down from "Rename" is "Delete" at index 4: the disabled row, the rule and the heading are all passed over rather than stopped on.
    route(&mut tree, &named(NamedKey::ArrowDown));
    route(&mut tree, &named(NamedKey::Enter));
    assert_eq!(
        seen.get(),
        Some(4),
        "three unreachable rows sit between the two that are not"
    );
}

/// A disabled row does not commit when it is clicked either, which is the half a keyboard test cannot see.
#[test]
fn a_disabled_row_does_not_commit_on_a_tap() {
    crate::test_support::fresh_layout_runtime();
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let structured = Children::new(|| {
        let mut slots = Slots::new();
        for (label, disabled) in [("Rename", false), ("Duplicate", true)] {
            slots.push(
                None,
                crate::list::item(
                    crate::list::ItemProps::props()
                        .label(Reactive::of(move || label.to_string()))
                        .disabled(Reactive::of(move || disabled))
                        .build(),
                    Children::default(),
                )?,
            );
        }
        Ok(slots)
    });
    let item = menu(
        MenuProps::props()
            .label("Actions")
            .on_select(Rc::new(move |i| sink.set(Some(i))))
            .build(),
        structured,
    )
    .unwrap();
    compute_layout(
        item.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(item);
    let _ = tree.commands();

    let tx = (panel_width() / 2.0) as f64;
    let ty = (crate::shared::control_height() / 2.0) as f64;
    route(&mut tree, &press(tx, ty));
    route(&mut tree, &release(tx, ty));
    relayout_if_dirty();

    let oy = (crate::shared::control_height()
        + crate::shared::popup_offset()
        + panel_pad()
        + row_height()
        + row_height() / 2.0) as f64;
    route(&mut tree, &press(tx, oy));
    route(&mut tree, &release(tx, oy));
    assert_eq!(seen.get(), None, "a disabled row commits nothing");
}

fn char_key(c: char) -> Event {
    Event::KeyPressed {
        key: telar::Key::Char(c),
        modifiers: telar::ModifiersState::default(),
        unmodified: None,
    }
}

/// Rename · Duplicate · Delete · [disabled] Deploy. Three rows share a first letter and the fourth is out of reach, which is the whole of what type-ahead has to tell apart.
fn typeahead_menu(sink: Rc<Cell<Option<u32>>>) -> Box<dyn LayoutItem> {
    let structured = Children::new(|| {
        let mut slots = Slots::new();
        for (label, disabled) in [
            ("Rename", false),
            ("Duplicate", false),
            ("Delete", false),
            ("Deploy", true),
        ] {
            slots.push(
                None,
                crate::list::item(
                    crate::list::ItemProps::props()
                        .label(Reactive::of(move || label.to_string()))
                        .disabled(Reactive::of(move || disabled))
                        .build(),
                    Children::default(),
                )?,
            );
        }
        Ok(slots)
    });
    let item = menu(
        MenuProps::props()
            .label("Actions")
            .on_select(Rc::new(move |i| sink.set(Some(i))))
            .build(),
        structured,
    )
    .unwrap();
    compute_layout(
        item.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    item
}

/// Opens a menu from the keyboard, types `typed`, commits, and reports the index that committed.
fn typed_pick(typed: &str) -> Option<u32> {
    use telar::NamedKey;

    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    ui_core::reset_keyboard();
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));

    let mut tree = telar::testing::scoped(|| ComponentList::new(typeahead_menu(seen.clone())));

    telar::focus::focus_next();
    route(&mut tree, &named(NamedKey::ArrowDown));
    // The rows are built on this flush, so nothing can be searched until it has run.
    relayout_if_dirty();
    for c in typed.chars() {
        route(&mut tree, &char_key(c));
    }
    route(&mut tree, &named(NamedKey::Enter));

    drop(tree);
    seen.get()
}

/// The plain case, and the one a list of more than a screenful is unusable without: a letter takes the cursor to the row that starts with it instead of making the user arrow there.
#[test]
fn a_typed_letter_moves_the_cursor_to_the_row_it_names() {
    assert_eq!(typed_pick("d"), Some(1), "`d` lands on Duplicate");
}

/// Refining holds still. `de` after `d` narrows towards a row rather than asking for the next one, which is why a multi-character query does not skip where the cursor already is.
#[test]
fn a_longer_query_narrows_instead_of_advancing() {
    assert_eq!(typed_pick("de"), Some(2), "`de` narrows past Duplicate");
}

/// A repeated letter cycles. `ddd` is not a query — no label could match it — it is "the next row starting with d", and it is the only way to reach the second of two rows sharing a first letter. The third press wraps back around, stepping over the disabled `Deploy` on the way: a row the keyboard may not stop on is not a search result either.
#[test]
fn a_repeated_letter_cycles_through_the_rows_that_share_it() {
    assert_eq!(
        typed_pick("dd"),
        Some(2),
        "the second `d` advances to Delete"
    );
    assert_eq!(
        typed_pick("ddd"),
        Some(1),
        "the third wraps past the disabled Deploy, back to Duplicate"
    );
}

/// A chord is a command, not a query. Without this, an application-level `Ctrl+S` reaching an open menu would silently walk its cursor to the first row starting with `s`.
#[test]
fn a_modified_character_is_not_a_search() {
    use telar::NamedKey;

    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    ui_core::reset_keyboard();
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let mut tree = ComponentList::new(typeahead_menu(seen.clone()));

    telar::focus::focus_next();
    route(&mut tree, &named(NamedKey::ArrowDown));
    relayout_if_dirty();
    route(
        &mut tree,
        &telar::testing::key_with(
            telar::Key::Char('d'),
            telar::ModifiersState {
                is_ctrl: true,
                ..Default::default()
            },
        ),
    );
    route(&mut tree, &named(NamedKey::Enter));
    ui_core::reset_keyboard();

    assert_eq!(
        seen.get(),
        Some(0),
        "the cursor stayed on Rename, where opening put it"
    );
}

/// A menu of three rows in a 400px square, laid out and drawn, with nothing focused.
fn mounted_menu() -> ComponentList {
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let item = menu(
        MenuProps::props().label("Actions").build(),
        rows(&["Rename", "Duplicate", "Delete"]),
    )
    .unwrap();
    compute_layout(
        item.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let tree = ComponentList::new(item);
    settle(&tree);
    tree
}

fn settle(tree: &ComponentList) {
    for _ in 0..3 {
        relayout_if_dirty();
        let _ = tree.commands();
    }
}

fn exposed(role: telar::focus::Role) -> Option<telar::focus::FocusId> {
    telar::focus::exposed()
        .into_iter()
        .find(|exposed| exposed.role == role)
        .map(|exposed| exposed.id)
}

/// A menu button hands the keyboard to the menu it opens, which points a reader at the row its cursor is on; the trigger names that menu as what it controls.
#[test]
fn an_open_menu_takes_the_keyboard_into_its_list() {
    use telar::NamedKey;

    let mut tree = mounted_menu();
    telar::focus::focus_next();
    let trigger = telar::focus::current().expect("the trigger is a stop");
    route(&mut tree, &named(NamedKey::ArrowDown));
    settle(&tree);

    let list = exposed(telar::focus::Role::Menu).expect("the list is open");
    assert_eq!(telar::focus::current(), Some(list));
    assert_eq!(telar::focus::controls_state(trigger), Some(list));
    assert!(
        telar::focus::exposed()
            .iter()
            .all(|row| row.role != telar::focus::Role::Option
                && row.role != telar::focus::Role::ListBox),
        "a menu's rows are actions, not options of a list box"
    );
    assert!(
        telar::focus::active_descendant_state(list).is_some(),
        "the list points at the row its cursor is on"
    );
    telar::focus::focus_next();
    assert_eq!(
        telar::focus::current(),
        Some(list),
        "Tab stays in the menu, the one stop it has"
    );
}

/// However the menu goes — Escape, a pick, a press away — the keyboard comes back to the trigger, which is where it was before the menu took it.
#[test]
fn every_way_of_closing_a_menu_gives_the_keyboard_back_to_its_trigger() {
    use telar::NamedKey;

    let mut tree = mounted_menu();
    telar::focus::focus_next();
    let trigger = telar::focus::current().expect("the trigger is a stop");

    route(&mut tree, &named(NamedKey::Enter));
    settle(&tree);
    route(&mut tree, &named(NamedKey::Escape));
    settle(&tree);
    assert_eq!(telar::focus::current(), Some(trigger), "after Escape");
    assert!(telar::focus::is_focus_visible(trigger));
    assert_eq!(telar::focus::controls_state(trigger), None, "shut");

    route(&mut tree, &named(NamedKey::ArrowDown));
    settle(&tree);
    route(&mut tree, &named(NamedKey::Enter));
    settle(&tree);
    assert_eq!(telar::focus::current(), Some(trigger), "after a pick");

    let (tx, ty) = (
        (panel_width() / 2.0) as f64,
        (crate::shared::control_height() / 2.0) as f64,
    );
    route(&mut tree, &press(tx, ty));
    route(&mut tree, &release(tx, ty));
    settle(&tree);
    assert_eq!(
        telar::focus::current(),
        exposed(telar::focus::Role::Menu),
        "a menu opened by a press takes the keyboard too"
    );
    let away = (390.0, 390.0);
    telar::focus::blur_from_pointer(away.0 as f32, away.1 as f32);
    route(&mut tree, &press(away.0, away.1));
    route(&mut tree, &release(away.0, away.1));
    settle(&tree);
    assert_eq!(telar::focus::current(), Some(trigger), "after a press away");
    assert!(
        !telar::focus::is_focus_visible(trigger),
        "and without a ring, as the press that opened it left it"
    );
}

/// Where a trigger's label is drawn: the clip it is cut by, which is the label's own box.
fn label_box(tree: &ComponentList) -> telar::Rect {
    tree.commands()
        .iter()
        .find_map(|c| match c {
            telar::DrawCommand::PushClip { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("the trigger clips its label")
}

/// The space a trigger leaves before its label and after it.
fn insets(tree: &ComponentList) -> (f32, f32) {
    let trigger = crate::test_support::drawn(telar::focus::Role::Button)[0];
    let label = label_box(tree);
    (
        label.x - trigger.x,
        (trigger.x + trigger.width) - (label.x + label.width),
    )
}

/// The same trigger measured to the glyphs it draws rather than to the box they are set in: the label's box was its line rounded up to a whole pixel, so the trigger sized by it had up to a pixel more after the text than before it.
#[test]
fn a_trigger_without_a_caret_pads_its_drawn_label_evenly_on_both_sides() {
    crate::test_support::fresh_layout_runtime();
    let item = menu(
        MenuProps::props()
            .label("Settings")
            .caret(false)
            .fit(true)
            .build(),
        rows(&["New"]),
    )
    .unwrap();
    let root = telar::new_container(
        telar::LayoutStyle::new()
            .flex_row()
            .width(400.0)
            .height(80.0),
        &[item.layout_node()],
    )
    .unwrap();
    telar::compute_layout(
        root,
        telar::AvailableSpace::Definite(400.0),
        telar::AvailableSpace::Definite(80.0),
    )
    .unwrap();
    telar::relayout_if_dirty();
    let tree = telar::ComponentList::new(item);
    let trigger = crate::test_support::drawn(telar::focus::Role::Button)[0];
    let (before, after) = crate::test_support::run_insets(&tree, "Settings", trigger);
    let pad = crate::shared::control_pad_x();
    assert!((before - pad).abs() <= 0.5, "{before} {after}");
    assert!((after - pad).abs() <= 0.5, "{before} {after}");
}

/// The workshop's "Settings" trigger read `[ Settings   ]`: with no caret its label was the only child of a `SPACE_BETWEEN` row, which puts a lone child at the start, so every pixel the trigger had beyond its label fell after it. Without a caret the label is the whole face, and stands centred in it as a button's does.
#[test]
fn a_trigger_without_a_caret_pads_its_label_evenly_on_both_sides() {
    let tree = laid_out_menu(true);
    let (before, after) = insets(&tree);
    assert_eq!(
        before,
        crate::shared::control_pad_x(),
        "fitted: {before} {after}"
    );
    assert_eq!(after, before, "fitted: {before} {after}");
    drop(tree);

    let tree = laid_out_menu(false);
    let (before, after) = insets(&tree);
    assert!(
        (before - after).abs() < 0.5,
        "a fixed-width trigger centres it too: {before} {after}"
    );
    assert!(before > crate::shared::control_pad_x(), "{before}");
}

#[derive(Clone)]
struct Themed {
    control: f32,
    menus: Option<MenuTriggerTokens>,
}

impl telar::ThemeTokens for Themed {
    fn radius_control(&self) -> f32 {
        self.control
    }
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(menus) = self.menus {
            extensions.insert(menus);
        }
    }
}

fn bordered_trigger(tree: &ComponentList) -> telar::RectStyle {
    tree.commands()
        .iter()
        .find_map(|c| match c {
            telar::DrawCommand::Rect { style, .. } if style.border.is_some() => Some(**style),
            _ => None,
        })
        .expect("a bordered trigger is painted")
}

#[test]
fn the_trigger_follows_the_control_role_until_the_trigger_tokens_name_their_own() {
    use telar::{Border, BorderRadius};

    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Themed {
        control: 7.0,
        menus: None,
    });
    let item = menu(
        MenuProps::props().label("File").bordered(true).build(),
        rows(&["New"]),
    )
    .unwrap();
    let root = telar::new_container(
        LayoutStyle::new().flex_row().width(400.0).height(80.0),
        &[item.layout_node()],
    )
    .unwrap();
    compute_layout(
        root,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(80.0),
    )
    .unwrap();
    let tree = ComponentList::new(item);
    let trigger = bordered_trigger(&tree);
    assert_eq!(trigger.radius, BorderRadius::all(7.0));
    assert_eq!(
        trigger.border.map(|border| border.widths),
        Some([crate::shared::border_width(); 4])
    );
    let width = crate::test_support::drawn(telar::focus::Role::Button)[0].width;
    assert_eq!(width, crate::dropdown::width(None));

    telar::set_theme(Themed {
        control: 7.0,
        menus: Some(MenuTriggerTokens {
            radius: Some(2.0),
            border_width: Some(3.0),
            width: Some(150.0),
            ..MenuTriggerTokens::default()
        }),
    });
    relayout_if_dirty();
    let trigger = bordered_trigger(&tree);
    assert_eq!(
        trigger.radius,
        BorderRadius::all(2.0),
        "the theme's own menu radius wins"
    );
    assert_eq!(
        trigger.border.map(|border: Border| border.widths),
        Some([3.0; 4])
    );
    let width = crate::test_support::drawn(telar::focus::Role::Button)[0].width;
    assert_eq!(width, 150.0, "and its width, laid out again on the switch");
}

/// A plain row per label, which is what the markup `menu … item label:"…"` compiles to. Enough for the tests that care about the trigger and the panel rather than about what a row can be.
fn rows(labels: &'static [&'static str]) -> Children {
    Children::new(move || {
        let mut slots = Slots::new();
        for label in labels {
            slots.push(
                None,
                crate::list::item(
                    crate::list::ItemProps::props()
                        .label(Reactive::of(move || label.to_string()))
                        .build(),
                    Children::default(),
                )?,
            );
        }
        Ok(slots)
    })
}

/// The shape of a trigger is the caller's call, not the component's.
///
/// The default splits them by what they are — a menu is a button, a select is a field — but a menu dropped into a row of inputs wants to match them, and there has to be a way to say so that is not editing the catalogue. Guards the props rather than the pixels: what matters is that they *reach* the trigger's paint at all.
#[test]
fn a_menu_can_be_asked_for_a_field_and_for_no_caret() {
    use telar::DrawCommand;
    use telar::{ComponentList, LayoutItem};

    let strokes = |bordered: bool, caret: bool| {
        crate::test_support::fresh_layout_runtime();
        let item = menu(
            MenuProps::props()
                .label("File")
                .bordered(bordered)
                .caret(caret)
                .build(),
            rows(&["New"]),
        )
        .unwrap();
        // Laid out first: the caret is drawn by a `Canvas`, which has nothing to draw into until it has a rect.
        let root = telar::new_container(
            telar::LayoutStyle::new().width(300.0).height(80.0),
            &[item.layout_node()],
        )
        .unwrap();
        telar::compute_layout(
            root,
            telar::AvailableSpace::Definite(300.0),
            telar::AvailableSpace::Definite(80.0),
        )
        .unwrap();
        let tree = ComponentList::new(item);
        let cmds = tree.commands().to_vec();
        let bordered_boxes = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::Rect { style, .. } if style.border.is_some()))
            .count();
        let paths = cmds
            .iter()
            .filter(|c| matches!(c, DrawCommand::Path { .. }))
            .count();
        (bordered_boxes, paths)
    };

    let (plain_border, plain_caret) = strokes(false, true);
    assert_eq!(
        plain_border, 0,
        "a menu is a button, so no frame by default"
    );
    assert_eq!(plain_caret, 1, "and it says it opens something");

    let (asked_border, _) = strokes(true, true);
    assert_eq!(asked_border, 1, "a caller that wants the field gets it");

    let (_, no_caret) = strokes(false, false);
    assert_eq!(no_caret, 0, "and one that wants no caret gets none");
}

/// A caller can restyle the surface without editing the catalogue.
///
/// The pressure this relieves is real: an app wanted its menu trigger squared off, and the only lever the catalogue offered was the theme's radius — which moves every rounded thing in the application. Editing `dropdown.rs` to hold one app's opinion is how a shared component stops being shared.
///
/// The amendment must *compose*, not replace: the component still decides that a bordered trigger wears a stroke, and the caller only says what they came to say.
#[test]
fn a_caller_can_amend_the_paint_the_trigger_worked_out_for_itself() {
    use telar::{BorderRadius, DrawCommand, ShapeStyle};
    use telar::{ComponentList, LayoutItem};

    crate::test_support::fresh_layout_runtime();
    let item = menu(
        MenuProps::props()
            .label("File")
            .bordered(true)
            .style(Rc::new(|s| {
                s.with_radius(BorderRadius::all(0.0))
                    .with_fill(Color::rgba(1.0, 0.0, 0.0, 1.0))
            }))
            .build(),
        rows(&["New"]),
    )
    .unwrap();
    let root = telar::new_container(
        telar::LayoutStyle::new().width(300.0).height(80.0),
        &[item.layout_node()],
    )
    .unwrap();
    telar::compute_layout(
        root,
        telar::AvailableSpace::Definite(300.0),
        telar::AvailableSpace::Definite(80.0),
    )
    .unwrap();
    let tree = ComponentList::new(item);
    let trigger = tree
        .commands()
        .iter()
        .find_map(|c| match c {
            DrawCommand::Rect { style, .. } if style.border.is_some() => Some(style.clone()),
            _ => None,
        })
        .expect("a bordered trigger is painted");

    assert_eq!(
        trigger.radius,
        BorderRadius::all(0.0),
        "the caller's radius reaches the paint"
    );
    assert_eq!(
        trigger.fill,
        Some(telar::Paint::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0))),
        "and so does their fill"
    );
    assert!(
        trigger.border.is_some(),
        "while the component keeps the border it decided a field wears"
    );
}

fn laid_out_menu(fit: bool) -> telar::testing::Scoped<telar::ComponentList> {
    crate::test_support::fresh_layout_runtime();
    telar::testing::scoped(|| {
        let item = menu(
            MenuProps::props()
                .label("File")
                .caret(false)
                .fit(fit)
                .build(),
            rows(&["New"]),
        )
        .unwrap();
        let root = telar::new_container(
            telar::LayoutStyle::new()
                .flex_row()
                .width(400.0)
                .height(80.0),
            &[item.layout_node()],
        )
        .unwrap();
        telar::compute_layout(
            root,
            telar::AvailableSpace::Definite(400.0),
            telar::AvailableSpace::Definite(80.0),
        )
        .unwrap();
        telar::relayout_if_dirty();
        telar::ComponentList::new(item)
    })
}

#[test]
fn a_fitted_menu_takes_the_width_of_its_label_rather_than_the_fixed_one() {
    let width = |fit: bool| {
        let _tree = laid_out_menu(fit);
        crate::test_support::drawn(telar::focus::Role::Button)[0].width
    };
    assert_eq!(width(false), panel_width());
    assert!(width(true) < panel_width() / 2.0, "{}", width(true));
}

#[test]
fn a_menu_trigger_is_washed_while_the_pointer_rests_on_it() {
    use telar::DrawCommand;
    use telar::testing::{centre, moved, route};

    let mut tree = laid_out_menu(true);
    let filled = |tree: &telar::ComponentList| {
        tree.commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::Rect { style, .. } if style.fill.is_some()))
            .count()
    };
    assert_eq!(filled(&tree), 0);
    let (x, y) = centre(crate::test_support::drawn(telar::focus::Role::Button)[0]);
    route(&mut tree, &moved(x, y));
    assert_eq!(filled(&tree), 1);
    route(&mut tree, &moved(390.0, 79.0));
    assert_eq!(filled(&tree), 0);
}

/// A menu of three rows whose trigger stands `top` px down a 400 × 300 window, opened from the keyboard, with the trigger's box and the panel's list.
fn opened_under(
    top: f32,
    size: telar::ControlSize,
    tokens: MenuTokens,
) -> (telar::Rect, telar::Rect, f32, f32) {
    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let (mut offset, mut pad) = (0.0, 0.0);
    let mut tree = telar::testing::scoped(|| {
        let item = telar::provide_control_size(size, || {
            offset = crate::shared::popup_offset();
            pad = panel_pad();
            menu(
                MenuProps::props().label("File").tokens(tokens).build(),
                rows(&["New", "Open", "Close"]),
            )
        })
        .unwrap();
        let root = telar::Container::new(
            LayoutStyle::new().flex_column().padding_top(top),
            vec![telar::box_item(item)],
        )
        .unwrap();
        compute_layout(
            root.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(300.0),
        )
        .unwrap();
        ComponentList::new(root)
    });
    settle(&tree);
    telar::focus::focus_next();
    route(&mut tree, &named(telar::NamedKey::Enter));
    settle(&tree);
    let trigger = crate::test_support::drawn(telar::focus::Role::Button)[0];
    let list = crate::test_support::drawn(telar::focus::Role::Menu)[0];
    (trigger, list, offset, pad)
}

/// The panel stands the theme's popup offset away from its trigger — under it, or over it once it flips — so it reads as a surface the button opened rather than one grown out of it.
#[test]
fn the_panel_keeps_the_popup_offset_from_its_trigger_on_either_side() {
    use telar::ControlSize;

    let offset = crate::shared::popup_offset();
    assert!(offset > 0.0);
    let (trigger, list, _, _) = opened_under(0.0, ControlSize::Regular, MenuTokens::default());
    assert_eq!(
        list.y - panel_pad(),
        trigger.y + trigger.height + offset,
        "under it"
    );

    let (trigger, list, _, _) = opened_under(240.0, ControlSize::Regular, MenuTokens::default());
    assert_eq!(
        list.y + list.height + panel_pad(),
        trigger.y - offset,
        "over it, flipped"
    );
}

/// The offset is a length of the control size in force, and the menu's tokens name one of their own at the regular size.
#[test]
fn the_popup_offset_scales_with_the_control_size_and_follows_the_tokens() {
    use telar::ControlSize;

    let gap = |size: ControlSize, tokens: MenuTokens| {
        let (trigger, list, offset, pad) = opened_under(0.0, size, tokens);
        (list.y - pad - (trigger.y + trigger.height), offset)
    };
    let (large, offset) = gap(ControlSize::Large, MenuTokens::default());
    assert_eq!(large, offset, "the offset at the size in force");
    assert!(large > gap(ControlSize::Regular, MenuTokens::default()).0);
    let own = MenuTokens {
        offset: Some(20.0),
        ..MenuTokens::default()
    };
    assert_eq!(gap(ControlSize::Regular, own).0, 20.0);
}

/// A row's shortcut is written once, portably, and shown the way the platform writes it: on a PC, `Mod` is Ctrl and the keys are words joined with `+`.
#[test]
fn a_row_s_shortcut_shows_as_the_platform_writes_it() {
    use telar::{DrawCommand, KeyboardConvention, NamedKey, set_keyboard_convention};

    let open_with = |convention: KeyboardConvention| {
        crate::test_support::fresh_layout_runtime();
        set_keyboard_convention(convention);
        telar::focus::clear();
        let item = menu(
            MenuProps::props().label("Edit").build(),
            Children::new(|| {
                let mut slots = Slots::new();
                for (label, shortcut) in [("Undo", "Mod+Z"), ("Redo", "Mod+Shift+Z")] {
                    slots.push(
                        None,
                        crate::list::item(
                            crate::list::ItemProps::props()
                                .label(label)
                                .shortcut(shortcut)
                                .build(),
                            Children::default(),
                        )?,
                    );
                }
                Ok(slots)
            }),
        )
        .unwrap();
        compute_layout(
            item.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        let mut tree = ComponentList::new(item);
        settle(&tree);
        telar::focus::focus_next();
        route(&mut tree, &named(NamedKey::ArrowDown));
        settle(&tree);
        let drawn: Vec<String> = tree
            .commands()
            .iter()
            .filter_map(|c| match c {
                DrawCommand::Text { text, .. } => Some(text.to_string()),
                _ => None,
            })
            .collect();
        let rows: Vec<_> = ui_core::accessibility::snapshot(&tree.commands())
            .into_iter()
            .filter(|node| node.role == telar::Role::MenuItem)
            .collect();
        let read: Vec<String> = rows.iter().map(|node| node.name.clone()).collect();
        let keys: Vec<Option<String>> = rows
            .into_iter()
            .map(|node| node.keyboard_shortcut)
            .collect();
        (drawn, read, keys)
    };

    let (drawn, read, keys) = open_with(KeyboardConvention::Pc);
    for shown in ["Ctrl+Z", "Ctrl+Shift+Z"] {
        assert!(drawn.iter().any(|text| text == shown), "{shown}: {drawn:?}");
    }
    assert!(
        read.iter().any(|name| name.contains("Ctrl+Z")),
        "a reader hears the shortcut as it is shown: {read:?}"
    );
    assert_eq!(
        keys,
        [
            Some("Control+Z".to_string()),
            Some("Control+Shift+Z".to_string())
        ],
        "and is told the keys in the form a reader takes"
    );

    let (drawn, _, keys) = open_with(KeyboardConvention::Mac);
    for shown in ["⌘Z", "⇧⌘Z"] {
        assert!(drawn.iter().any(|text| text == shown), "{shown}: {drawn:?}");
    }
    assert_eq!(
        keys,
        [Some("Meta+Z".to_string()), Some("Shift+Meta+Z".to_string())],
        "a Mac names Command, not Control"
    );
}

/// A row can come and go while its menu is open: hidden, it takes no room and the keyboard passes over it; shown, it is there without the menu opening again.
#[test]
fn a_row_comes_and_goes_with_what_it_acts_on_while_the_menu_is_open() {
    use telar::NamedKey;

    crate::test_support::fresh_layout_runtime();
    telar::focus::clear();
    let shown = signal(false);
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let structured = Children::new(move || {
        let mut slots = Slots::new();
        let row = |label: &'static str, shown: Reactive<bool>| {
            crate::list::item(
                crate::list::ItemProps::props()
                    .label(Reactive::of(move || label.to_string()))
                    .shown(shown)
                    .build(),
                Children::default(),
            )
        };
        slots.push(None, row("Rename", Reactive::of(|| true))?);
        slots.push(
            None,
            crate::list::separator(
                crate::list::SeparatorProps::props()
                    .shown(Reactive::of(move || shown.get()))
                    .build(),
                Children::default(),
            )?,
        );
        slots.push(None, row("Restore", Reactive::of(move || shown.get()))?);
        slots.push(None, row("Delete", Reactive::of(|| true))?);
        Ok(slots)
    });
    let item = menu(
        MenuProps::props()
            .label("Actions")
            .on_select(Rc::new(move |i| sink.set(Some(i))))
            .build(),
        structured,
    )
    .unwrap();
    compute_layout(
        item.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(item);
    telar::focus::focus_next();
    route(&mut tree, &named(NamedKey::ArrowDown));
    settle(&tree);
    assert!(!telar::testing::texts(&tree).contains(&"Restore".to_string()));

    shown.set(true);
    settle(&tree);
    assert!(
        telar::testing::texts(&tree).contains(&"Restore".to_string()),
        "shown without opening the menu again"
    );
    shown.set(false);
    settle(&tree);
    assert!(!telar::testing::texts(&tree).contains(&"Restore".to_string()));
    route(&mut tree, &named(NamedKey::ArrowDown));
    route(&mut tree, &named(NamedKey::Enter));
    assert_eq!(
        seen.get(),
        Some(3),
        "the hidden rule and row are passed over"
    );
}
