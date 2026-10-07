use std::cell::RefCell;
use std::rc::Rc;

use telar::{Component, ComponentList, DrawCommand, LayoutItem, NamedKey, Rect, signal};

use super::*;
use crate::harness::{named, press, release};

const SIZE: f32 = 20.0;

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
    ui_core::reset_keyboard();
    telar::focus::clear();
    let sink = seen.clone();
    let item = swatches(
        SwatchesProps::props()
            .colors(palette())
            .names(vec!["Red".to_string(), "Green".to_string()])
            .selected(selected)
            .size(SIZE)
            .on_select(Rc::new(move |i| sink.borrow_mut().push(i)))
            .build(),
        Children::default(),
    )
    .unwrap();
    let rect = crate::harness::lay_out_row(item.layout_node(), 400.0, 100.0);
    (item, rect)
}

fn swatch_centre(row: Rect, index: u32) -> (f64, f64) {
    let step = SIZE + shared::spacing() * 0.5;
    (
        (row.x + index as f32 * step + SIZE / 2.0) as f64,
        (row.y + SIZE / 2.0) as f64,
    )
}

fn tap(item: &mut Box<dyn LayoutItem>, at: (f64, f64)) {
    item.on_event(&press(at.0, at.1));
    item.on_event(&release(at.0, at.1));
}

fn focus_swatch(index: u32) {
    for _ in 0..=index {
        telar::focus::focus_next();
    }
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
fn arrows_move_the_selection_and_clamp_at_both_ends() {
    let selected = signal(Some(2u32));
    let seen = Rc::default();
    let (mut item, _) = build(selected, &seen);
    focus_swatch(2);

    key(&mut item, NamedKey::ArrowRight);
    assert_eq!(selected.peek(), Some(3));
    key(&mut item, NamedKey::ArrowDown);
    assert_eq!(selected.peek(), Some(3), "clamped at the last swatch");

    key(&mut item, NamedKey::ArrowLeft);
    key(&mut item, NamedKey::ArrowUp);
    assert_eq!(selected.peek(), Some(1));
    key(&mut item, NamedKey::ArrowLeft);
    key(&mut item, NamedKey::ArrowLeft);
    assert_eq!(selected.peek(), Some(0), "clamped at the first swatch");
    assert_eq!(seen.borrow().first(), Some(&3));
}

#[test]
fn home_and_end_select_the_first_and_last() {
    let selected = signal(Some(1u32));
    let seen = Rc::default();
    let (mut item, _) = build(selected, &seen);
    focus_swatch(1);

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

    assert_eq!(borders, vec![1.0, 2.0, 1.0, 1.0]);
}

#[test]
fn empty_colours_build_an_empty_row() {
    crate::test_support::fresh_layout_runtime();
    let item = swatches(SwatchesProps::props().build(), Children::default()).unwrap();
    let _ = item.view();
}
