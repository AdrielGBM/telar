use std::cell::RefCell;
use std::rc::Rc;

use telar::testing::{named, press, release};
use telar::{Component, ComponentList, ConsumedKeys, DrawCommand, LayoutItem, NamedKey, Rect, signal};

use super::*;

const DIAMETER: f32 = 20.0;

fn palette() -> Vec<Color> {
    vec![
        Color::rgba(1.0, 0.0, 0.0, 1.0),
        Color::rgba(0.0, 1.0, 0.0, 1.0),
        Color::rgba(0.0, 0.0, 1.0, 1.0),
        Color::rgba(1.0, 1.0, 0.0, 1.0),
    ]
}

fn build(
    selected: RwSignal<Option<u32>>,
    seen: &Rc<RefCell<Vec<u32>>>,
) -> (Box<dyn LayoutItem>, Rect) {
    crate::test_support::fresh_layout_runtime();
    telar::testing::reset_keyboard();
    telar::focus::clear();
    let sink = seen.clone();
    let item = swatches(
        SwatchesProps::props()
            .colors(palette())
            .names(vec!["Red".to_string(), "Green".to_string()])
            .selected(selected)
            .diameter(DIAMETER)
            .on_select(Rc::new(move |i| sink.borrow_mut().push(i)))
            .build(),
    )
    .unwrap();
    let rect = telar::testing::lay_out_row(item.layout_node(), 400.0, 100.0);
    (item, rect)
}

fn swatch_centre(row: Rect, index: u32) -> (f64, f64) {
    let hit = DIAMETER.max(shared::MIN_TARGET);
    let step = hit + (shared::gap_sm(ControlSize::Regular) - (hit - DIAMETER)).max(0.0);
    (
        (row.x + index as f32 * step + hit / 2.0) as f64,
        (row.y + hit / 2.0) as f64,
    )
}

fn tap(item: &mut Box<dyn LayoutItem>, at: (f64, f64)) {
    item.on_event(&press(at.0, at.1));
    item.on_event(&release(at.0, at.1));
}

fn focused_swatch() -> Option<usize> {
    let focused = telar::focus::current()?;
    telar::focus::exposed()
        .into_iter()
        .filter(|exposed| exposed.role == Role::Radio)
        .position(|exposed| exposed.id == focused)
}

fn key(item: &mut Box<dyn LayoutItem>, k: NamedKey) {
    item.on_event(&named(k));
}

#[test]
fn pressing_the_third_swatch_selects_index_two_and_reports_it() {
    let selected = signal(None::<u32>);
    let seen = Rc::default();
    let (mut item, row) = build(selected, &seen);

    tap(&mut item, swatch_centre(row, 2));

    assert_eq!(selected.peek(), Some(2));
    assert_eq!(*seen.borrow(), vec![2]);
}

#[test]
fn an_unselected_state_stays_unselected_until_pressed() {
    let selected = signal(None::<u32>);
    let seen = Rc::default();
    let (mut item, row) = build(selected, &seen);
    let _ = item.view();

    assert_eq!(selected.peek(), None);
    assert!(seen.borrow().is_empty());

    tap(&mut item, swatch_centre(row, 0));
    assert_eq!(selected.peek(), Some(0));
}

#[test]
fn the_row_is_one_tab_stop_landing_on_the_chosen_swatch() {
    let (_item, _) = build(signal(Some(2u32)), &Rc::default());
    telar::focus::focus_next();
    assert_eq!(focused_swatch(), Some(2));
    let stops = telar::focus::tab_order()
        .into_iter()
        .filter(|stop| stop.role == Role::Radio)
        .count();
    assert_eq!(stops, 1);
}

#[test]
fn the_arrows_move_focus_and_the_selection_and_wrap_at_both_ends() {
    let selected = signal(Some(2u32));
    let seen = Rc::default();
    let (mut item, _) = build(selected, &seen);
    telar::focus::focus_next();

    key(&mut item, NamedKey::ArrowRight);
    assert_eq!((selected.peek(), focused_swatch()), (Some(3), Some(3)));
    key(&mut item, NamedKey::ArrowDown);
    assert_eq!(
        (selected.peek(), focused_swatch()),
        (Some(0), Some(0)),
        "past the last swatch comes the first"
    );
    key(&mut item, NamedKey::ArrowLeft);
    assert_eq!(selected.peek(), Some(3), "and before the first, the last");
    key(&mut item, NamedKey::ArrowUp);
    assert_eq!((selected.peek(), focused_swatch()), (Some(2), Some(2)));
    assert_eq!(*seen.borrow(), vec![3, 0, 3, 2]);
}

/// A right-to-left row lays its first swatch out on the right, so Right walks back toward it.
#[test]
fn right_in_a_right_to_left_surface_moves_to_the_swatch_laid_out_on_its_right() {
    let selected = signal(Some(2u32));
    let (mut item, _) = build(selected, &Rc::default());
    telar::set_direction(telar::Direction::Rtl);
    telar::focus::focus_next();
    key(&mut item, NamedKey::ArrowRight);
    telar::set_direction(telar::Direction::Ltr);
    assert_eq!((selected.peek(), focused_swatch()), (Some(1), Some(1)));
}

#[test]
fn the_swatches_are_radios_of_one_group() {
    let (item, _) = build(signal(Some(1u32)), &Rc::default());
    let nodes = telar::inspect::snapshot(&ComponentList::new(item).commands());
    let group = nodes
        .iter()
        .position(|node| node.role == Role::RadioGroup)
        .unwrap_or_else(|| panic!("a radio group: {nodes:?}"));
    let radios: Vec<_> = nodes
        .iter()
        .filter(|node| node.role == Role::Radio)
        .map(|node| node.parent)
        .collect();
    assert_eq!(radios, vec![Some(group); 4]);
}

#[test]
fn home_and_end_select_the_first_and_last() {
    let selected = signal(Some(1u32));
    let seen = Rc::default();
    let (mut item, _) = build(selected, &seen);
    telar::focus::focus_next();

    key(&mut item, NamedKey::End);
    assert_eq!(selected.peek(), Some(3));
    key(&mut item, NamedKey::Home);
    assert_eq!(selected.peek(), Some(0));
    assert_eq!(*seen.borrow(), vec![3, 0]);
}

#[test]
fn the_selected_swatch_draws_a_thicker_border_than_the_others() {
    let selected = signal(Some(1u32));
    let (item, _) = build(selected, &Rc::default());

    let tree = ComponentList::new(item);
    let borders: Vec<f32> = tree
        .commands()
        .iter()
        .filter_map(|c| match c {
            DrawCommand::Rect { style, .. } => style.border.as_ref().map(|b| b.widths[0]),
            _ => None,
        })
        .collect();

    let (hairline, strong) = (shared::border_width(), shared::border_width_strong());
    assert_eq!(borders, vec![hairline, strong, hairline, hairline]);
}

#[test]
fn empty_colours_build_an_empty_row() {
    crate::test_support::fresh_layout_runtime();
    let item = swatches(SwatchesProps::props().build()).unwrap();
    let _ = item.view();
}

#[test]
fn a_swatch_smaller_than_a_pointer_target_is_pressed_from_the_margin_around_it() {
    let selected = signal(None::<u32>);
    let (mut item, row) = build(selected, &Rc::default());
    let margin = (shared::MIN_TARGET - DIAMETER) / 4.0;

    tap(
        &mut item,
        ((row.x + margin) as f64, (row.y + margin) as f64),
    );

    assert_eq!(selected.peek(), Some(0));
}

#[derive(Clone, Default)]
struct Plain {
    swatches: Option<SwatchesTokens>,
}

impl telar::ThemeTokens for Plain {
    fn register_extensions(&self, extensions: &mut telar::ThemeExtensions) {
        if let Some(swatches) = &self.swatches {
            extensions.insert(swatches.clone());
        }
    }
}

fn faces(
    size: ControlSize,
    style: Option<telar::SurfaceAmendment>,
) -> Vec<(Rect, telar::RectStyle)> {
    crate::test_support::fresh_layout_runtime();
    let mut props = SwatchesProps::props()
        .colors(palette())
        .size(size)
        .selected(signal(Some(0u32)))
        .build();
    props.style = style;
    telar::testing::scoped(|| {
        let item = swatches(props).unwrap();
        telar::testing::lay_out_row(item.layout_node(), 400.0, 100.0);
        ComponentList::new(item)
    })
    .commands()
    .iter()
    .filter_map(|c| match c {
        DrawCommand::Rect { rect, style } if style.border.is_some() => Some((*rect, **style)),
        _ => None,
    })
    .collect()
}

#[test]
fn an_unsized_swatch_is_an_icon_and_a_step_across_and_follows_the_tokens() {
    telar::set_theme(Plain::default());
    let drawn = faces(ControlSize::Regular, None);
    let side = shared::icon_size(ControlSize::Regular) + shared::gap_sm(ControlSize::Regular);
    assert_eq!((drawn[0].0.width, drawn[0].0.height), (side, side));
    assert_eq!(drawn[0].1.radius, telar::BorderRadius::all(side / 2.0));

    telar::set_theme(Plain {
        swatches: Some(SwatchesTokens {
            size: Some(28.0),
            ring: Some(3.0),
        }),
    });
    let drawn = faces(ControlSize::Regular, None);
    assert_eq!(drawn[0].0.width, 28.0);
    assert_eq!(drawn[0].1.border.map(|b| b.widths[0]), Some(3.0));
}

/// A row's size scales its swatches, the tokens' diameter with them, while each stays pressed from at least the smallest target.
#[test]
fn the_size_scales_the_swatches() {
    for size in [ControlSize::Small, ControlSize::Large] {
        telar::set_theme(Plain::default());
        let side = shared::icon_size(size) + shared::gap_sm(size);
        assert_eq!(faces(size, None)[0].0.width, side, "{size:?}");

        telar::set_theme(Plain {
            swatches: Some(SwatchesTokens {
                size: Some(28.0),
                ring: None,
            }),
        });
        assert_eq!(
            faces(size, None)[0].0.width,
            28.0 * size.scale(),
            "{size:?}"
        );
    }
}

#[test]
fn a_style_amendment_reshapes_every_swatch() {
    telar::set_theme(Plain::default());
    let square: telar::SurfaceAmendment =
        Rc::new(|s: telar::RectStyle| s.with_radius(telar::BorderRadius::all(0.0)));
    let drawn = faces(ControlSize::Regular, Some(square));
    assert_eq!(drawn.len(), palette().len());
    assert!(
        drawn
            .iter()
            .all(|(_, style)| style.radius == telar::BorderRadius::all(0.0))
    );
}

/// With none chosen, Tab lands on the first swatch, which Space or Enter picks.
#[test]
fn enter_and_space_pick_the_focused_swatch() {
    for activation in [NamedKey::Enter, NamedKey::Space] {
        let selected = signal(None::<u32>);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let (mut item, _) = build(selected, &seen);
        telar::focus::focus_next();
        key(&mut item, activation.clone());
        assert_eq!(selected.get(), Some(0), "{activation:?}");
        assert_eq!(*seen.borrow(), vec![0], "{activation:?}");
    }
}

#[test]
fn every_swatch_keeps_the_arrows_the_edges_and_its_activation_keys() {
    let keys = crate::test_support::declared_keys(Role::Radio, || {
        let (item, _) = build(signal(None::<u32>), &Rc::new(RefCell::new(Vec::new())));
        ComponentList::new(item)
    });
    assert_eq!(
        keys,
        vec![ConsumedKeys::ARROWS | ConsumedKeys::EDGES | ConsumedKeys::ACTIVATION; 4],
    );
}

#[test]
fn every_swatch_is_colour_content_a_document_keeps_the_colours_of() {
    crate::test_support::fresh_layout_runtime();
    let was = telar::testing::set_element_capture(true);
    let tree = telar::testing::scoped(|| {
        let item = swatches(
            SwatchesProps::props()
                .colors(palette())
                .diameter(DIAMETER)
                .build(),
        )
        .unwrap();
        telar::testing::lay_out_row(item.layout_node(), 400.0, 100.0);
        ComponentList::new(item)
    });
    let kept = tree
        .commands()
        .iter()
        .filter(|command| {
            matches!(
                command,
                DrawCommand::PushElement { element }
                    if element.forced_colors == telar::ForcedColors::Keep
            )
        })
        .count();
    telar::testing::set_element_capture(was);
    assert_eq!(kept, palette().len());
}
