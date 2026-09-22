use std::cell::Cell;
use std::rc::Rc;

use geometry_core::Size;
use layout_reactive::set_surface_size;
use reactive_core::effect;

use super::*;

fn columns() -> Breakpoints<u32> {
    breakpoint(1).at(1200.0, 3).at(600.0, 2)
}

#[test]
fn each_width_picks_the_widest_step_it_has_reached() {
    let columns = columns();
    assert_eq!(*columns.value_at(0.0), 1);
    assert_eq!(*columns.value_at(599.9), 1);
    assert_eq!(
        *columns.value_at(600.0),
        2,
        "a threshold belongs to the step it starts"
    );
    assert_eq!(*columns.value_at(1199.0), 2);
    assert_eq!(*columns.value_at(4000.0), 3);
}

#[test]
fn a_second_step_at_the_same_width_replaces_the_first() {
    let columns = breakpoint(1).at(600.0, 2).at(600.0, 5);
    assert_eq!(*columns.value_at(700.0), 5);
    assert_eq!(columns.range_at(700.0), 1);
}

#[test]
fn a_followed_value_moves_only_across_a_threshold() {
    set_surface_size(Size::new(320.0, 640.0));
    let followed = columns().follow();
    let runs = Rc::new(Cell::new(0));
    let seen = Rc::new(Cell::new(0));
    let (counted, read) = (runs.clone(), seen.clone());
    let _reader = effect(move || {
        read.set(followed.get());
        counted.set(counted.get() + 1);
    });
    assert_eq!((seen.get(), runs.get()), (1, 1));

    for width in [330.0, 480.0, 599.0] {
        set_surface_size(Size::new(width, 640.0));
    }
    assert_eq!(runs.get(), 1, "every width so far is in the first range");

    set_surface_size(Size::new(800.0, 640.0));
    assert_eq!((seen.get(), runs.get()), (2, 2));
    set_surface_size(Size::new(800.0, 300.0));
    set_surface_size(Size::new(1000.0, 300.0));
    assert_eq!(
        runs.get(),
        2,
        "a height change and a move within the range are not crossings"
    );

    set_surface_size(Size::new(1300.0, 300.0));
    assert_eq!((seen.get(), runs.get()), (3, 3));
}

#[test]
fn two_ranges_with_the_same_value_are_not_a_change() {
    set_surface_size(Size::new(100.0, 100.0));
    let followed = breakpoint(8.0f32).at(500.0, 8.0).at(900.0, 16.0).follow();
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    let _reader = effect(move || {
        followed.get();
        counted.set(counted.get() + 1);
    });
    set_surface_size(Size::new(600.0, 100.0));
    assert_eq!(runs.get(), 1);
}
