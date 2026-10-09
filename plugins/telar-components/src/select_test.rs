use telar::testing::{press, release, route};
use telar::{
    AvailableSpace, ComponentList, compute_layout, dispatch_overlays, relayout_if_dirty, signal,
};

use super::*;

/// Three choices as `item` rows, which is how a select is written now.
fn sizes() -> Children {
    Children::new(|| {
        let mut slots = Slots::new();
        for label in ["Small", "Medium", "Large"] {
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

#[test]
fn builds_and_lays_out() {
    crate::test_support::fresh_layout_runtime();
    let picked = signal(1u32);
    let item = select(SelectProps::props().selected(picked).build(), sizes()).unwrap();
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
        "closed select is at least the trigger tall: {:?}",
        root_rect.get()
    );
    let tree = ComponentList::new(item);
    let _ = tree.commands();
}

#[test]
fn selecting_an_option_sets_the_signal_and_closes() {
    use std::cell::Cell;
    use std::rc::Rc;

    crate::test_support::fresh_layout_runtime();
    let picked = signal(0u32);
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let item = select(
        SelectProps::props()
            .selected(picked)
            .on_select(Rc::new(move |i| sink.set(Some(i))))
            .build(),
        sizes(),
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

    let ox = PANEL_WIDTH / 2.0;
    let oy = (TRIGGER_HEIGHT + panel_pad() + 2.0 * ROW_HEIGHT + ROW_HEIGHT / 2.0) as f64;
    route(&mut tree, &press(ox as f64, oy));
    route(&mut tree, &release(ox as f64, oy));

    assert_eq!(
        picked.get(),
        2,
        "picking the third option sets the signal to 2"
    );
    assert_eq!(seen.get(), Some(2), "on_select fires with the picked index");
    assert!(
        !dispatch_overlays(&press(ox as f64, oy)),
        "the panel closes after a selection"
    );
}

/// The constraint that kept a select on a flat `options` prop, met head on: the trigger says the chosen row's own label *before the panel has ever been opened*, which is the only moment where there are no rows to read it from. The declaring walk is what supplies it.
#[test]
fn the_trigger_names_the_chosen_row_before_the_panel_has_ever_opened() {
    crate::test_support::fresh_layout_runtime();
    let picked = signal(1u32);
    let item = select(SelectProps::props().selected(picked).build(), sizes()).unwrap();
    compute_layout(
        item.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let tree = ComponentList::new(item);

    assert!(
        drawn_text(&tree).iter().any(|t| t == "Medium"),
        "the trigger reads the label off row 1 without the panel existing: {:?}",
        drawn_text(&tree)
    );
}

/// And what the flat prop could never say. A row is an ordinary component with ordinary props, so one of them can be disabled — and a disabled row is not a place the keyboard stops or a tap commits.
#[test]
fn a_choice_can_be_disabled_which_a_list_of_strings_could_not_say() {
    use std::cell::Cell;
    use std::rc::Rc;

    crate::test_support::fresh_layout_runtime();
    let picked = signal(0u32);
    let seen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let sink = seen.clone();
    let rows = Children::new(|| {
        let mut slots = Slots::new();
        for (label, disabled) in [("Small", false), ("Medium", true), ("Large", false)] {
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
    let item = select(
        SelectProps::props()
            .selected(picked)
            .on_select(Rc::new(move |i| sink.set(Some(i))))
            .build(),
        rows,
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
    route(&mut tree, &press(tx, (TRIGGER_HEIGHT / 2.0) as f64));
    route(&mut tree, &release(tx, (TRIGGER_HEIGHT / 2.0) as f64));
    relayout_if_dirty();

    let oy = (TRIGGER_HEIGHT + panel_pad() + ROW_HEIGHT + ROW_HEIGHT / 2.0) as f64;
    route(&mut tree, &press(tx, oy));
    route(&mut tree, &release(tx, oy));

    assert_eq!(seen.get(), None, "a disabled choice commits nothing");
    assert_eq!(picked.get(), 0, "and leaves the bound signal alone");
}

/// Every string the tree draws, for asserting on what a trigger says.
fn drawn_text(tree: &ComponentList) -> Vec<String> {
    tree.commands()
        .iter()
        .filter_map(|c| match c {
            telar::DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

/// Choices of very different lengths, so a trigger sized from what it shows would change width with the pick.
fn uneven() -> Children {
    Children::new(|| {
        let mut slots = Slots::new();
        for label in ["S", "Medium", "An extraordinarily long choice"] {
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

/// The rects drawn, in draw order: the trigger's comes first.
fn drawn_rects(tree: &ComponentList) -> Vec<telar::Rect> {
    tree.commands()
        .iter()
        .filter_map(|c| match c {
            telar::DrawCommand::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect()
}

/// A stretched select in a fixed-width row is as wide as the row whatever it shows. Sized from its label, it widened past the row as soon as a longer choice was picked from the panel.
#[test]
fn a_stretched_select_keeps_the_width_of_its_row_after_a_longer_pick() {
    use telar::LayoutStyle;
    crate::test_support::fresh_layout_runtime();
    let picked = signal(0u32);
    let item = select(
        SelectProps::props().selected(picked).stretch(true).build(),
        uneven(),
    )
    .unwrap();
    let field = item.layout_node();
    let field_rect = track_layout(field).unwrap();
    let row = telar::new_container(LayoutStyle::new().flex_row().width(136.0), &[field]).unwrap();
    let mut tree = ComponentList::new(item);
    compute_layout(
        row,
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let _ = tree.commands();
    assert_eq!(field_rect.get().width, 136.0, "before the pick");

    let ty = (TRIGGER_HEIGHT / 2.0) as f64;
    route(&mut tree, &press(20.0, ty));
    route(&mut tree, &release(20.0, ty));
    relayout_if_dirty();
    let oy = (TRIGGER_HEIGHT + panel_pad() + 2.0 * ROW_HEIGHT + ROW_HEIGHT / 2.0) as f64;
    route(&mut tree, &press(20.0, oy));
    route(&mut tree, &release(20.0, oy));
    relayout_if_dirty();

    assert_eq!(picked.get(), 2, "the long choice was picked");
    assert_eq!(field_rect.get().width, 136.0, "after the pick");
    assert_eq!(
        drawn_rects(&tree)[0].width,
        136.0,
        "the trigger is the row's width"
    );
}

/// What a stretched select shows plays no part in its share of the row, so two of them split it evenly.
#[test]
fn stretched_selects_share_a_row_evenly_whatever_they_show() {
    use telar::LayoutStyle;
    crate::test_support::fresh_layout_runtime();
    let short = select(
        SelectProps::props()
            .selected(signal(0u32))
            .stretch(true)
            .build(),
        uneven(),
    )
    .unwrap();
    let long = select(
        SelectProps::props()
            .selected(signal(2u32))
            .stretch(true)
            .build(),
        uneven(),
    )
    .unwrap();
    let (short_rect, long_rect) = (
        track_layout(short.layout_node()).unwrap(),
        track_layout(long.layout_node()).unwrap(),
    );
    telar::new_container(
        LayoutStyle::new().flex_row().width(400.0),
        &[short.layout_node(), long.layout_node()],
    )
    .and_then(|row| {
        compute_layout(
            row,
            AvailableSpace::Definite(400.0),
            AvailableSpace::Definite(400.0),
        )
    })
    .unwrap();

    assert_eq!(short_rect.get().width, 200.0);
    assert_eq!(long_rect.get().width, 200.0);
}

/// A row taller than the trigger stretches the select's box, not the trigger: growing the trigger to fill the row's width grew it down the column it sits in.
#[test]
fn a_stretched_trigger_keeps_its_height_in_a_taller_row() {
    use telar::LayoutStyle;
    crate::test_support::fresh_layout_runtime();
    let item = select(SelectProps::props().stretch(true).build(), sizes()).unwrap();
    let row = telar::new_container(
        LayoutStyle::new().flex_row().width(160.0).height(120.0),
        &[item.layout_node()],
    )
    .unwrap();
    let tree = ComponentList::new(item);
    compute_layout(
        row,
        AvailableSpace::Definite(160.0),
        AvailableSpace::Definite(120.0),
    )
    .unwrap();

    let trigger = drawn_rects(&tree)[0];
    assert_eq!((trigger.width, trigger.height), (160.0, TRIGGER_HEIGHT));
}

/// A row that sizes to its content has no width to offer, so there a stretched select is as wide as what it shows rather than collapsing.
#[test]
fn a_stretched_select_in_a_content_sized_row_is_as_wide_as_what_it_shows() {
    use telar::LayoutStyle;
    crate::test_support::fresh_layout_runtime();
    let item = select(
        SelectProps::props()
            .selected(signal(2u32))
            .stretch(true)
            .build(),
        uneven(),
    )
    .unwrap();
    let field_rect = track_layout(item.layout_node()).unwrap();
    let hugging =
        telar::new_container(LayoutStyle::new().flex_row(), &[item.layout_node()]).unwrap();
    let row = telar::new_container(
        LayoutStyle::new()
            .flex_row()
            .align_items(telar::AlignItems::CENTER)
            .width(600.0),
        &[hugging],
    )
    .unwrap();
    compute_layout(
        row,
        AvailableSpace::Definite(600.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();

    assert!(
        field_rect.get().width > PANEL_WIDTH,
        "the long choice and the caret fit: {:?}",
        field_rect.get()
    );
}
