use std::cell::Cell;
use std::rc::Rc;

use reactive_core::effect;

use super::*;

fn runs_of(read: impl Fn() + 'static) -> Rc<Cell<u32>> {
    let runs = Rc::new(Cell::new(0));
    let counted = runs.clone();
    let _ = effect(move || {
        read();
        counted.set(counted.get() + 1);
    });
    runs
}

#[test]
fn a_resize_is_read_back_at_once() {
    set_surface_size(Size::new(320.0, 480.0));
    assert_eq!(surface_size(), Size::new(320.0, 480.0));
}

#[test]
fn a_width_reader_sleeps_through_a_height_change() {
    set_surface_size(Size::new(320.0, 480.0));
    let width_runs = runs_of(|| {
        use_surface_width();
    });
    let size_runs = runs_of(|| {
        use_surface_size();
    });
    set_surface_size(Size::new(320.0, 400.0));
    assert_eq!(width_runs.get(), 1, "only the height moved");
    assert_eq!(size_runs.get(), 2);
    set_surface_size(Size::new(360.0, 400.0));
    assert_eq!(width_runs.get(), 2);
}

#[test]
fn the_same_size_again_wakes_nobody() {
    set_surface_size(Size::new(100.0, 100.0));
    let runs = runs_of(|| {
        use_surface_size();
    });
    set_surface_size(Size::new(100.0, 100.0));
    assert_eq!(runs.get(), 1);
}

#[test]
fn both_sides_moving_is_one_notification() {
    set_surface_size(Size::new(100.0, 100.0));
    let runs = runs_of(|| {
        use_surface_size();
    });
    set_surface_size(Size::new(200.0, 300.0));
    assert_eq!(runs.get(), 2);
}

#[test]
fn each_surface_keeps_its_own_size() {
    let first = SurfaceSizeContext::new();
    let second = SurfaceSizeContext::new();
    {
        let _in = first.enter();
        set_surface_size(Size::new(800.0, 600.0));
    }
    {
        let _in = second.enter();
        set_surface_size(Size::new(80.0, 24.0));
    }
    let _in = first.enter();
    assert_eq!(surface_size(), Size::new(800.0, 600.0));
}
