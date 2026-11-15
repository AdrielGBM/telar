use i18n_core::Direction;
use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{Event, Key, ModifiersState, NamedKey};
use reactive_core::{RwSignal, signal};
use renderer_core::RectStyle;

use super::*;
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime, set_direction};
use crate::focus::{
    Role, clear, current, focus_next, focus_prev, focusable_of, request_from_pointer,
};
use crate::layout_item::{LayoutItem, box_item};
use crate::styled_container::StyledContainer;
use ui_tree::ComponentList;

fn control() -> StyledContainer {
    StyledContainer::new(
        LayoutStyle::new().width(40.0).height(30.0),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
}

/// A field, a row of three radios choosing into `chosen`, and another field, laid out and mounted.
struct Page {
    tree: ComponentList,
    before: FocusId,
    items: Vec<FocusId>,
    after: FocusId,
}

impl Page {
    fn new(axis: RovingAxis, chosen: RwSignal<Option<usize>>) -> Self {
        reset_layout_runtime();
        clear();
        let group = RovingGroup::new(axis, move || chosen.get(), move |at| chosen.set(Some(at)));
        let items: Vec<StyledContainer> = (0..3)
            .map(|at| control().control(Role::Radio).roving(&group, at))
            .collect();
        let ids = items.iter().filter_map(StyledContainer::focus_id).collect();
        let before = control().control(Role::Button);
        let after = control().control(Role::Button);
        let (before_id, after_id) = (before.focus_id().unwrap(), after.focus_id().unwrap());
        let row = Container::new(
            LayoutStyle::row(),
            items.into_iter().map(box_item).collect(),
        )
        .unwrap();
        let page = Container::new(
            LayoutStyle::new().flex_column(),
            vec![box_item(before), box_item(row), box_item(after)],
        )
        .unwrap();
        compute_layout(
            page.layout_node(),
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
        .unwrap();
        Self {
            tree: ComponentList::new(page),
            before: before_id,
            items: ids,
            after: after_id,
        }
    }

    fn key(&mut self, key: NamedKey) {
        self.tree.on_event(&Event::KeyPressed {
            key: Key::Named(key),
            modifiers: ModifiersState::default(),
            unmodified: None,
        });
    }
}

#[test]
fn tab_enters_the_group_once_on_the_chosen_item_and_leaves_it_on_the_next_press() {
    let page = Page::new(RovingAxis::Horizontal, signal(Some(1)));
    focus_next();
    assert_eq!(current(), Some(page.before));
    focus_next();
    assert_eq!(
        current(),
        Some(page.items[1]),
        "Tab lands on the chosen item"
    );
    focus_next();
    assert_eq!(
        current(),
        Some(page.after),
        "and the next Tab leaves the group"
    );
    focus_prev();
    assert_eq!(
        current(),
        Some(page.items[1]),
        "Shift+Tab comes back to the chosen item"
    );
    clear();
}

#[test]
fn with_nothing_chosen_tab_lands_on_the_first_item() {
    let page = Page::new(RovingAxis::Horizontal, signal(None));
    focus_next();
    focus_next();
    assert_eq!(current(), Some(page.items[0]));
    clear();
}

#[test]
fn the_arrows_move_focus_and_choose_the_item_they_reach_wrapping_at_either_end() {
    let chosen = signal(Some(0));
    let mut page = Page::new(RovingAxis::Horizontal, chosen);
    focus_next();
    focus_next();
    page.key(NamedKey::ArrowRight);
    assert_eq!((current(), chosen.get()), (Some(page.items[1]), Some(1)));
    page.key(NamedKey::End);
    assert_eq!((current(), chosen.get()), (Some(page.items[2]), Some(2)));
    page.key(NamedKey::ArrowRight);
    assert_eq!(
        (current(), chosen.get()),
        (Some(page.items[0]), Some(0)),
        "past the last item comes the first"
    );
    page.key(NamedKey::ArrowDown);
    assert_eq!(
        current(),
        Some(page.items[0]),
        "a row is not walked with Down"
    );
    focus_next();
    assert_eq!(
        current(),
        Some(page.after),
        "Tab leaves from the item reached"
    );
    clear();
}

#[test]
fn a_group_on_both_axes_takes_down_as_toward_the_last_item() {
    let chosen = signal(Some(0));
    let mut page = Page::new(RovingAxis::Both, chosen);
    focus_next();
    focus_next();
    page.key(NamedKey::ArrowDown);
    assert_eq!((current(), chosen.get()), (Some(page.items[1]), Some(1)));
    page.key(NamedKey::ArrowUp);
    assert_eq!((current(), chosen.get()), (Some(page.items[0]), Some(0)));
    clear();
}

/// A right-to-left row lays its first item out on the right, so Right walks back toward it.
#[test]
fn right_in_a_right_to_left_surface_moves_to_the_item_laid_out_on_its_right() {
    let chosen = signal(Some(1));
    let mut page = Page::new(RovingAxis::Horizontal, chosen);
    set_direction(Direction::Rtl);
    focus_next();
    focus_next();
    page.key(NamedKey::ArrowRight);
    let reached = (current(), chosen.get());
    page.key(NamedKey::ArrowLeft);
    let back = (current(), chosen.get());
    set_direction(Direction::Ltr);
    assert_eq!(reached, (Some(page.items[0]), Some(0)));
    assert_eq!(back, (Some(page.items[1]), Some(1)));
    clear();
}

/// A press gives focus to the item pressed, chosen or not, and that item is where Tab leaves from.
#[test]
fn an_item_given_focus_is_the_group_s_stop() {
    let page = Page::new(RovingAxis::Horizontal, signal(Some(0)));
    request_from_pointer(page.items[2]);
    assert_eq!(current(), Some(page.items[2]));
    focus_next();
    assert_eq!(current(), Some(page.after));
    focus_prev();
    assert_eq!(
        current(),
        Some(page.items[0]),
        "once focus is outside, the chosen item is the stop again"
    );
    clear();
}

/// What a host walking its own tab order is told: one item of the group is a stop.
#[test]
fn only_the_stop_is_a_tab_stop_to_a_host() {
    let page = Page::new(RovingAxis::Horizontal, signal(Some(2)));
    let stops: Vec<bool> = page
        .items
        .iter()
        .map(|&id| focusable_of(id, Role::Radio, None).tab_stop)
        .collect();
    assert_eq!(stops, [false, false, true]);
    clear();
}

/// An item built again for its place, as a tooltip may build what it wraps, takes that place over.
#[test]
fn an_item_built_again_takes_its_place_over() {
    reset_layout_runtime();
    clear();
    let chosen = signal(Some(0));
    let group = RovingGroup::new(
        RovingAxis::Horizontal,
        move || chosen.get(),
        move |at| chosen.set(Some(at)),
    );
    let first = control().control(Role::Radio).roving(&group, 0);
    let again = control().control(Role::Radio).roving(&group, 0);
    drop(first);
    let second = control().control(Role::Radio).roving(&group, 1);
    let ids = [again.focus_id(), second.focus_id()];
    let _page =
        Container::new(LayoutStyle::row(), vec![box_item(again), box_item(second)]).unwrap();
    focus_next();
    assert_eq!(current(), ids[0]);
    clear();
}
