use telar::testing::{press, release, route};
use telar::{
    AvailableSpace, ComponentList, compute_layout, dispatch_overlays, relayout_if_dirty, signal,
};

use super::*;

/// Three choices as `item` rows, which is how a select is written now.
fn sizes() -> Children {
    Children::new(|| {
        let mut items = Vec::new();
        for label in ["Small", "Medium", "Large"] {
            items.push(crate::list::item(
                crate::list::ItemProps::props()
                    .label(Reactive::of(move || label.to_string()))
                    .build(),
            )?);
        }
        Ok(items)
    })
}

#[test]
fn builds_and_lays_out() {
    crate::test_support::fresh_layout_runtime();
    let picked = signal(1u32);
    let item = select(
        SelectProps::props()
            .selected(picked)
            .children(sizes())
            .build(),
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
            .children(sizes())
            .build(),
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

    let ox = panel_width() / 2.0;
    let oy = (crate::shared::control_height()
        + crate::shared::popup_offset()
        + panel_pad()
        + 2.0 * row_height()
        + row_height() / 2.0) as f64;
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
    let item = select(
        SelectProps::props()
            .selected(picked)
            .children(sizes())
            .build(),
    )
    .unwrap();
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
        let mut items = Vec::new();
        for (label, disabled) in [("Small", false), ("Medium", true), ("Large", false)] {
            items.push(crate::list::item(
                crate::list::ItemProps::props()
                    .label(Reactive::of(move || label.to_string()))
                    .disabled(Reactive::of(move || disabled))
                    .build(),
            )?);
        }
        Ok(items)
    });
    let item = select(
        SelectProps::props()
            .selected(picked)
            .on_select(Rc::new(move |i| sink.set(Some(i))))
            .children(rows)
            .build(),
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
    route(
        &mut tree,
        &press(tx, (crate::shared::control_height() / 2.0) as f64),
    );
    route(
        &mut tree,
        &release(tx, (crate::shared::control_height() / 2.0) as f64),
    );
    relayout_if_dirty();

    let oy = (crate::shared::control_height()
        + crate::shared::popup_offset()
        + panel_pad()
        + row_height()
        + row_height() / 2.0) as f64;
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
        let mut items = Vec::new();
        for label in ["S", "Medium", "An extraordinarily long choice"] {
            items.push(crate::list::item(
                crate::list::ItemProps::props()
                    .label(Reactive::of(move || label.to_string()))
                    .build(),
            )?);
        }
        Ok(items)
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
        SelectProps::props()
            .selected(picked)
            .stretch(true)
            .children(uneven())
            .build(),
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

    let ty = (crate::shared::control_height() / 2.0) as f64;
    route(&mut tree, &press(20.0, ty));
    route(&mut tree, &release(20.0, ty));
    relayout_if_dirty();
    let oy = (crate::shared::control_height()
        + crate::shared::popup_offset()
        + panel_pad()
        + 2.0 * row_height()
        + row_height() / 2.0) as f64;
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
            .children(uneven())
            .build(),
    )
    .unwrap();
    let long = select(
        SelectProps::props()
            .selected(signal(2u32))
            .stretch(true)
            .children(uneven())
            .build(),
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
    let item = select(SelectProps::props().stretch(true).children(sizes()).build()).unwrap();
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
    assert_eq!(
        (trigger.width, trigger.height),
        (160.0, crate::shared::control_height())
    );
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
            .children(uneven())
            .build(),
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
        field_rect.get().width > panel_width(),
        "the long choice and the caret fit: {:?}",
        field_rect.get()
    );
}

fn many(count: u32) -> Children {
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

/// A 25-row panel is 800px; in a 300px window it has to be cut to the room and scroll, with the keyboard cursor kept on screen.
#[test]
fn a_tall_panel_is_capped_to_the_window_and_follows_the_keyboard() {
    crate::test_support::fresh_layout_runtime();
    let picked = signal(0u32);
    let item = select(
        SelectProps::props()
            .selected(picked)
            .children(many(25))
            .build(),
    )
    .unwrap();
    compute_layout(
        item.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(300.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(item);
    let _ = tree.commands();

    let settle = |tree: &ComponentList| {
        for _ in 0..3 {
            relayout_if_dirty();
            let _ = tree.commands();
        }
    };
    let down = telar::testing::named(telar::NamedKey::ArrowDown);

    telar::focus::focus_next();
    route(&mut tree, &down);
    settle(&tree);

    let panel = crate::test_support::drawn(telar::focus::Role::ListBox)[0];
    assert!(
        panel.y + panel.height + panel_pad() <= 300.0,
        "the panel stays inside the window: {panel:?}"
    );
    assert!(panel.height < 800.0, "and is cut to the room: {panel:?}");

    for _ in 0..24 {
        route(&mut tree, &down);
    }
    settle(&tree);

    let tx = (panel.x + panel.width / 2.0) as f64;
    let last_visible = (panel.y + panel.height - row_height() / 2.0) as f64;
    route(&mut tree, &press(tx, last_visible));
    route(&mut tree, &release(tx, last_visible));
    assert_eq!(
        picked.get(),
        24,
        "the cursor on the last row scrolled it to the bottom of the panel"
    );
}

#[derive(Clone)]
struct Themed {
    selects: Option<SelectTokens>,
    lists: Option<crate::ListTokens>,
}

impl telar::ThemeTokens for Themed {
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(selects) = self.selects {
            extensions.insert(selects);
        }
        if let Some(lists) = self.lists {
            extensions.insert(lists);
        }
    }
}

/// A select laid out at the origin of a 400px square, opened when `open` says so.
fn mounted(props: SelectProps, open: bool) -> ComponentList {
    let item = select(SelectProps {
        children: sizes(),
        ..props
    })
    .unwrap();
    compute_layout(
        item.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(item);
    let _ = tree.commands();
    if open {
        let (x, y) =
            telar::testing::centre(crate::test_support::drawn(telar::focus::Role::ComboBox)[0]);
        route(&mut tree, &press(x, y));
        route(&mut tree, &release(x, y));
        relayout_if_dirty();
        let _ = tree.commands();
    }
    tree
}

/// The paint of the first box drawn over exactly `rect`.
fn painted(tree: &ComponentList, rect: telar::Rect) -> telar::RectStyle {
    tree.commands()
        .iter()
        .find_map(|c| match c {
            telar::DrawCommand::Rect { rect: at, style } if *at == rect => Some(**style),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a box is painted over {rect:?}"))
}

#[test]
fn the_trigger_is_shaped_by_the_select_tokens_and_amended_by_its_caller() {
    use telar::{BorderRadius, Paint, ShapeStyle};

    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Themed {
        selects: Some(SelectTokens {
            radius: Some(3.0),
            border_width: Some(2.5),
            ..SelectTokens::default()
        }),
        lists: None,
    });
    let red = Color::rgba(1.0, 0.0, 0.0, 1.0);
    let tree = mounted(
        SelectProps::props()
            .style(Rc::new(move |s: telar::RectStyle| s.with_fill(red)))
            .build(),
        false,
    );
    let trigger = painted(
        &tree,
        crate::test_support::drawn(telar::focus::Role::ComboBox)[0],
    );
    assert_eq!(trigger.radius, BorderRadius::all(3.0));
    assert_eq!(trigger.border.map(|border| border.widths), Some([2.5; 4]));
    assert_eq!(
        trigger.fill,
        Some(Paint::Solid(red)),
        "the caller's fill wins"
    );
}

/// Every list's rows stand at one height, which the theme's list tokens move for all of them, and the chosen one wears the theme's selection wash rather than a strength of the select's own.
#[test]
fn the_rows_take_the_list_tokens_height_and_the_chosen_one_the_selection_wash() {
    use telar::Paint;

    crate::test_support::fresh_layout_runtime();
    telar::set_theme(Themed {
        selects: None,
        lists: Some(crate::ListTokens {
            row_height: Some(40.0),
            ..crate::ListTokens::default()
        }),
    });
    let tree = mounted(SelectProps::props().selected(signal(1u32)).build(), true);
    let options = crate::test_support::drawn(telar::focus::Role::Option);
    assert_eq!(options.len(), 3);
    assert!(options.iter().all(|row| row.height == 40.0), "{options:?}");
    let rows: Vec<telar::RectStyle> = tree
        .commands()
        .iter()
        .filter_map(|c| match c {
            telar::DrawCommand::Rect { rect, style }
                if rect.width == options[0].width && rect.height == options[0].height =>
            {
                Some(**style)
            }
            _ => None,
        })
        .collect();
    assert_eq!(rows.len(), 3, "{rows:?}");
    assert_eq!(rows[1].fill, Some(Paint::Solid(crate::shared::held_wash())));
    assert_eq!(rows[2].fill, None);
}
