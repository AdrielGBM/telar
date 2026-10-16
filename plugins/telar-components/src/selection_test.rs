use std::cell::RefCell;

use telar::{KeyboardConvention, set_keyboard_convention};

use super::*;

fn ids(names: &[&str]) -> Vec<Arc<str>> {
    names.iter().map(|name| Arc::from(*name)).collect()
}

fn chosen(many: RwSignal<BTreeSet<Arc<str>>>) -> Vec<String> {
    many.get().iter().map(|id| id.to_string()).collect()
}

fn several() -> (RowSelection, RwSignal<BTreeSet<Arc<str>>>) {
    crate::test_support::fresh_layout_runtime();
    let many = signal(BTreeSet::new());
    let selection = RowSelection::new(SelectionMode::Multiple, None, Some(many), None);
    (selection, many)
}

fn held(shift: bool, ctrl: bool, meta: bool) -> ModifiersState {
    ModifiersState {
        is_shift: shift,
        is_ctrl: ctrl,
        is_meta: meta,
        ..Default::default()
    }
}

#[test]
fn a_press_asks_for_what_its_modifiers_say_and_one_row_only_in_single_mode() {
    set_keyboard_convention(KeyboardConvention::Pc);
    let multiple = |modifiers| Gesture::of_press(SelectionMode::Multiple, modifiers);
    assert_eq!(multiple(held(false, false, false)), Gesture::Only);
    assert_eq!(multiple(held(false, true, false)), Gesture::Toggle);
    assert_eq!(
        multiple(held(true, false, false)),
        Gesture::Range { keep: false }
    );
    assert_eq!(
        multiple(held(true, true, false)),
        Gesture::Range { keep: true }
    );
    assert_eq!(
        multiple(held(false, false, true)),
        Gesture::Only,
        "Command is not the primary modifier on a PC"
    );
    set_keyboard_convention(KeyboardConvention::Mac);
    assert_eq!(multiple(held(false, false, true)), Gesture::Toggle);
    set_keyboard_convention(KeyboardConvention::Pc);
    assert_eq!(
        Gesture::of_press(SelectionMode::Single, held(true, true, false)),
        Gesture::Only
    );
}

#[test]
fn single_mode_keeps_one_row_whatever_the_gesture() {
    crate::test_support::fresh_layout_runtime();
    let one = signal(None);
    let selection = RowSelection::new(SelectionMode::Single, Some(one), None, None);
    let order = ids(&["a", "b", "c"]);
    selection.apply(Gesture::Only, &order[0], || order.clone(), None);
    selection.apply(Gesture::Toggle, &order[2], || order.clone(), None);
    assert_eq!(one.get().as_deref(), Some("c"));
    selection.all(&order);
    assert_eq!(one.get().as_deref(), Some("c"), "Ctrl+A chooses nothing");
    assert!(selection.is_selected("c"));
    assert!(!selection.is_selected("a"));
}

#[test]
fn a_toggle_adds_and_takes_away_and_a_range_runs_from_the_anchor() {
    let (selection, many) = several();
    let order = ids(&["a", "b", "c", "d", "e"]);
    selection.apply(Gesture::Only, &order[1], || order.clone(), None);
    selection.apply(Gesture::Toggle, &order[3], || order.clone(), None);
    assert_eq!(chosen(many), ["b", "d"]);
    selection.apply(
        Gesture::Range { keep: false },
        &order[0],
        || order.clone(),
        None,
    );
    assert_eq!(
        chosen(many),
        ["a", "b", "c", "d"],
        "from the row toggled last, back to the row pressed"
    );
    selection.apply(
        Gesture::Range { keep: false },
        &order[4],
        || order.clone(),
        None,
    );
    assert_eq!(chosen(many), ["d", "e"], "the anchor stays where it was");
    selection.apply(Gesture::Toggle, &order[0], || order.clone(), None);
    selection.apply(
        Gesture::Range { keep: true },
        &order[1],
        || order.clone(),
        None,
    );
    assert_eq!(chosen(many), ["a", "b", "d", "e"], "kept beside the others");
    selection.apply(Gesture::Toggle, &order[1], || order.clone(), None);
    assert_eq!(chosen(many), ["a", "d", "e"]);
}

#[test]
fn a_range_with_no_anchor_runs_from_the_cursor() {
    let (selection, many) = several();
    let order = ids(&["a", "b", "c", "d"]);
    selection.apply(
        Gesture::Range { keep: false },
        &order[3],
        || order.clone(),
        Some(&order[1]),
    );
    assert_eq!(chosen(many), ["b", "c", "d"]);
    selection.apply(
        Gesture::Range { keep: false },
        &order[2],
        || order.clone(),
        None,
    );
    assert_eq!(chosen(many), ["b", "c"], "and that row is the anchor after");
}

#[test]
fn all_chooses_every_row_and_then_none() {
    let (selection, many) = several();
    let order = ids(&["a", "b", "c"]);
    selection.apply(Gesture::Only, &order[1], || order.clone(), None);
    selection.all(&order);
    assert_eq!(chosen(many), ["a", "b", "c"]);
    selection.all(&order);
    assert!(chosen(many).is_empty());
}

#[test]
fn the_handler_hears_each_change_and_nothing_that_changes_nothing() {
    crate::test_support::fresh_layout_runtime();
    let heard = Rc::new(RefCell::new(Vec::new()));
    let sink = heard.clone();
    let selection = RowSelection::new(
        SelectionMode::Multiple,
        None,
        None,
        Some(Rc::new(move |rows: &BTreeSet<Arc<str>>| {
            sink.borrow_mut().push(rows.len())
        })),
    );
    let order = ids(&["a", "b"]);
    selection.apply(Gesture::Only, &order[0], || order.clone(), None);
    selection.apply(Gesture::Only, &order[0], || order.clone(), None);
    selection.all(&order);
    assert_eq!(*heard.borrow(), [1, 2]);
    assert_eq!(
        selection.first_of(order.iter().rev()).as_deref(),
        Some("b"),
        "the first chosen in the order asked"
    );
}
