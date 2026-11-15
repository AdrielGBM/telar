use std::cell::Cell;
use std::rc::Rc;

use super::*;

const ALIVE: SurfaceHandle = SurfaceHandle::from_raw(NonZeroU64::MIN);
const GONE: SurfaceHandle = SurfaceHandle::from_raw(NonZeroU64::MIN.saturating_add(1));

fn install_hook_knowing_only_alive_and_ambient() {
    set_surface_enter_hook(|handle, run| {
        if handle.is_none_or(|handle| handle == ALIVE) {
            with_current_surface(handle, run);
        } else {
            run();
        }
    });
}

#[test]
fn entering_if_alive_runs_inside_a_surface_the_hook_enters() {
    install_hook_knowing_only_alive_and_ambient();
    let seen = enter_surface_if_alive(Some(ALIVE), current_surface);
    assert_eq!(seen, Some(Some(ALIVE)));
    assert_eq!(current_surface(), None, "restored afterwards");
}

#[test]
fn entering_if_alive_skips_a_surface_that_is_gone() {
    install_hook_knowing_only_alive_and_ambient();
    let ran = Rc::new(Cell::new(false));
    let outcome = enter_surface_if_alive(Some(GONE), {
        let ran = Rc::clone(&ran);
        move || ran.set(true)
    });
    assert_eq!(outcome, None);
    assert!(!ran.get());
}

#[test]
fn entering_if_alive_runs_inside_the_surface_already_active() {
    install_hook_knowing_only_alive_and_ambient();
    let nested = enter_surface(Some(ALIVE), || enter_surface_if_alive(Some(ALIVE), || 7));
    assert_eq!(nested, Some(7));
}

#[test]
fn the_ambient_world_never_goes() {
    install_hook_knowing_only_alive_and_ambient();
    let seen = enter_surface(Some(ALIVE), || {
        enter_surface_if_alive(None, current_surface)
    });
    assert_eq!(seen, Some(None));
}

#[test]
fn with_current_surface_restores_the_one_before_when_it_unwinds() {
    let outcome =
        std::panic::catch_unwind(|| with_current_surface(Some(ALIVE), || panic!("unwinds")));
    assert!(outcome.is_err());
    assert_eq!(current_surface(), None);
}

struct EntersOnDrop(Rc<Cell<bool>>);

impl Drop for EntersOnDrop {
    fn drop(&mut self) {
        self.0.set(enter_surface(Some(ALIVE), || true));
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
    install_hook_knowing_only_alive_and_ambient();
    assert!(dropped.get());
}
