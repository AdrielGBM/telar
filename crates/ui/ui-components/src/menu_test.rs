use std::cell::Cell;
use std::rc::Rc;

use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::Event;
use ui_core::{ComponentList, EventResult, compute_layout, dispatch_overlays, relayout_if_dirty};

use super::*;
use crate::harness::{press, release, route};

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
    let row = ui_core::new_container(
        LayoutStyle::new().flex_row().width(44.0).height(400.0),
        &[root_node],
    )
    .unwrap();
    let mut tree = ComponentList::new(item);
    compute_layout(
        row,
        AvailableSpace::Definite(44.0),
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
            renderer_core::DrawCommand::Rect { rect, .. } => Some(rect.width),
            _ => None,
        })
        .collect();
    assert!(
        widths
            .iter()
            .any(|w| *w >= PANEL_WIDTH - panel_pad() * 2.0 - 0.5),
        "the open panel should be at least {PANEL_WIDTH}px wide, got {widths:?}"
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
        root_rect.get().height >= TRIGGER_HEIGHT - 0.5,
        "closed menu is at least the trigger tall: {:?}",
        root_rect.get()
    );
    let tree = ComponentList::new(item);
    let _ = tree.commands();
}

fn key(named: platform_core::NamedKey) -> Event {
    Event::KeyPressed {
        key: platform_core::Key::Named(named),
        modifiers: platform_core::ModifiersState::default(),
    }
}

/// A menu was reachable by mouse and by nothing else: the trigger took no focus, so Tab passed it by, and the panel answered to no key at all. Radix gives arrows, Home/End and Escape away for free; here every one of them was absent, which is the difference between a control a keyboard user can operate and one they cannot.
#[test]
fn a_menu_can_be_opened_and_driven_from_the_keyboard() {
    use platform_core::NamedKey;

    crate::test_support::fresh_layout_runtime();
    ui_core::focus::clear();
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

    ui_core::focus::focus_next();
    assert!(
        ui_core::focus::current().is_some(),
        "the trigger joined the tab order"
    );

    route(&mut tree, &key(NamedKey::ArrowDown));
    relayout_if_dirty();
    route(&mut tree, &key(NamedKey::ArrowDown));
    route(&mut tree, &key(NamedKey::Enter));
    assert_eq!(seen.get(), Some(1), "the highlighted row is what commits");
}

/// Escape closes it even though the trigger holds focus. `dispatch_overlays` only dismisses when nothing is focused — right for a field, which blurs itself first, and wrong here, where the focused thing *is* the control the menu belongs to.
#[test]
fn escape_closes_a_menu_whose_trigger_holds_focus() {
    use platform_core::NamedKey;

    crate::test_support::fresh_layout_runtime();
    ui_core::focus::clear();
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

    ui_core::focus::focus_next();
    route(&mut tree, &key(NamedKey::ArrowDown));
    relayout_if_dirty();
    route(&mut tree, &key(NamedKey::Escape));
    relayout_if_dirty();

    route(&mut tree, &key(NamedKey::Enter));
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

    let tx = (PANEL_WIDTH / 2.0) as f64;
    let ty = (TRIGGER_HEIGHT / 2.0) as f64;
    route(&mut tree, &press(tx, ty));
    route(&mut tree, &release(tx, ty));
    relayout_if_dirty();

    let ox = (PANEL_WIDTH / 2.0) as f64;
    let oy = (TRIGGER_HEIGHT + panel_pad() + ROW_HEIGHT + ROW_HEIGHT / 2.0) as f64;
    route(&mut tree, &press(ox, oy));
    route(&mut tree, &release(ox, oy));

    assert_eq!(
        seen.get(),
        Some(1),
        "picking the second item fires on_select(1)"
    );
    assert_eq!(
        dispatch_overlays(&press(ox, oy)),
        EventResult::Ignored,
        "the menu closes after a pick"
    );
}
/// What a menu could not say before, said end to end: a disabled row, a separator, and a heading are all *in* the list and none of them is a place the keyboard stops.
///
/// The three used to be inexpressible for the same reason — the rows were `Vec<&str>`, and a string carries no state — and they are testable together now for the same reason: each row builds itself inside the menu, so it can register what it is rather than being told.
#[test]
fn the_keyboard_steps_over_what_it_cannot_commit() {
    use platform_core::NamedKey;

    crate::test_support::fresh_layout_runtime();
    ui_core::focus::clear();
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

    ui_core::focus::focus_next();
    route(&mut tree, &key(NamedKey::ArrowDown));
    relayout_if_dirty();
    // One step down from "Rename" is "Delete" at index 4: the disabled row, the rule and the heading are all passed over rather than stopped on.
    route(&mut tree, &key(NamedKey::ArrowDown));
    route(&mut tree, &key(NamedKey::Enter));
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

    let tx = (PANEL_WIDTH / 2.0) as f64;
    let ty = (TRIGGER_HEIGHT / 2.0) as f64;
    route(&mut tree, &press(tx, ty));
    route(&mut tree, &release(tx, ty));
    relayout_if_dirty();

    let oy = (TRIGGER_HEIGHT + panel_pad() + ROW_HEIGHT + ROW_HEIGHT / 2.0) as f64;
    route(&mut tree, &press(tx, oy));
    route(&mut tree, &release(tx, oy));
    assert_eq!(seen.get(), None, "a disabled row commits nothing");
}

fn char_key(c: char) -> Event {
    Event::KeyPressed {
        key: platform_core::Key::Char(c),
        modifiers: platform_core::ModifiersState::default(),
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
    use platform_core::NamedKey;

    crate::test_support::fresh_layout_runtime();
    ui_core::focus::clear();
    ui_core::reset_keyboard();
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));

    // Scoped and disposed, because this helper runs twice in one test and the second call replaces the layout runtime, which starts its ids over. An unscoped first tree would keep effects running against ids the second now owns.
    let scope = reactive_core::owner_scope();
    let owner = scope.id();
    let mut tree = ComponentList::new(typeahead_menu(seen.clone()));

    ui_core::focus::focus_next();
    route(&mut tree, &key(NamedKey::ArrowDown));
    // The rows are built on this flush, so nothing can be searched until it has run.
    relayout_if_dirty();
    for c in typed.chars() {
        route(&mut tree, &char_key(c));
    }
    route(&mut tree, &key(NamedKey::Enter));

    drop(scope);
    drop(tree);
    reactive_core::dispose_owner(owner);
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
    use platform_core::NamedKey;

    crate::test_support::fresh_layout_runtime();
    ui_core::focus::clear();
    ui_core::reset_keyboard();
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let mut tree = ComponentList::new(typeahead_menu(seen.clone()));

    ui_core::focus::focus_next();
    route(&mut tree, &key(NamedKey::ArrowDown));
    relayout_if_dirty();
    ui_core::observe_keyboard(&Event::ModifiersChanged {
        modifiers: platform_core::ModifiersState {
            is_ctrl: true,
            ..Default::default()
        },
    });
    route(&mut tree, &char_key('d'));
    route(&mut tree, &key(NamedKey::Enter));
    ui_core::reset_keyboard();

    assert_eq!(
        seen.get(),
        Some(0),
        "the cursor stayed on Rename, where opening put it"
    );
}
