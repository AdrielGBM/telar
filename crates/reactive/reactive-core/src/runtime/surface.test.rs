use std::cell::Cell;
use std::rc::Rc;

use super::*;

const ALIVE: SurfaceHandle = SurfaceHandle::from_raw(1);
const GONE: SurfaceHandle = SurfaceHandle::from_raw(2);

fn install_hook_knowing_only_alive() {
    set_surface_enter_hook(|handle, run| {
        if handle == ALIVE {
            with_current_surface(handle, run);
        } else {
            run();
        }
    });
}

#[test]
fn with_entered_if_alive_runs_inside_a_surface_the_hook_enters() {
    install_hook_knowing_only_alive();
    let seen = ALIVE.with_entered_if_alive(current_surface);
    assert_eq!(seen, Some(ALIVE));
    assert_eq!(
        current_surface(),
        SurfaceHandle::NONE,
        "restored afterwards"
    );
}

#[test]
fn with_entered_if_alive_skips_a_surface_that_is_gone() {
    install_hook_knowing_only_alive();
    let ran = Rc::new(Cell::new(false));
    let outcome = GONE.with_entered_if_alive({
        let ran = Rc::clone(&ran);
        move || ran.set(true)
    });
    assert_eq!(outcome, None);
    assert!(!ran.get());
}

#[test]
fn with_entered_if_alive_runs_inside_the_surface_already_active() {
    install_hook_knowing_only_alive();
    let nested = ALIVE.with_entered(|| ALIVE.with_entered_if_alive(|| 7));
    assert_eq!(nested, Some(7));
}

#[test]
fn with_current_surface_restores_the_one_before_when_it_unwinds() {
    let outcome = std::panic::catch_unwind(|| with_current_surface(ALIVE, || panic!("unwinds")));
    assert!(outcome.is_err());
    assert_eq!(current_surface(), SurfaceHandle::NONE);
}

struct EntersOnDrop(Rc<Cell<bool>>);

impl Drop for EntersOnDrop {
    fn drop(&mut self) {
        self.0.set(ALIVE.with_entered(|| true));
    }
}

#[test]
fn a_replaced_hook_drops_after_the_hook_slot_is_released() {
    let dropped = Rc::new(Cell::new(false));
    let held = EntersOnDrop(Rc::clone(&dropped));
    set_surface_enter_hook(move |handle, run| {
        let _held = &held;
        with_current_surface(handle, run);
    });
    install_hook_knowing_only_alive();
    assert!(dropped.get());
}
