use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Route {
    Home,
    Settings,
    Detail,
}

#[test]
fn starts_at_root() {
    let nav = Navigator::new(Route::Home);
    assert_eq!(nav.current(), Route::Home);
    assert_eq!(nav.depth(), 1);
    assert!(!nav.can_pop(), "the root is the bottom of the stack");
}

#[test]
fn push_and_pop_track_history() {
    let nav = Navigator::new(Route::Home);
    nav.push(Route::Settings);
    assert_eq!(nav.current(), Route::Settings);
    assert_eq!(nav.depth(), 2);
    assert!(nav.can_pop(), "a pushed page can be popped");

    nav.push(Route::Detail);
    assert_eq!(nav.current(), Route::Detail);

    assert!(nav.pop(), "popping walks back one page");
    assert_eq!(nav.current(), Route::Settings);
    assert!(nav.pop(), "and again, down to the root");
    assert_eq!(nav.current(), Route::Home);
}

#[test]
fn pop_at_root_is_a_noop() {
    let nav = Navigator::new(Route::Home);
    assert!(!nav.pop(), "the root cannot be popped away");
    assert_eq!(nav.current(), Route::Home);
    assert_eq!(nav.depth(), 1);
}

#[test]
fn replace_swaps_top_without_growing() {
    let nav = Navigator::new(Route::Home);
    nav.push(Route::Settings);
    nav.replace(Route::Detail);
    assert_eq!(nav.current(), Route::Detail);
    assert_eq!(nav.depth(), 2);
}

#[test]
fn pop_to_root_and_reset_clear_the_stack() {
    let nav = Navigator::new(Route::Home);
    nav.push(Route::Settings);
    nav.push(Route::Detail);
    nav.pop_to_root();
    assert_eq!(nav.current(), Route::Home);
    assert_eq!(nav.depth(), 1);

    nav.push(Route::Settings);
    nav.reset(Route::Detail);
    assert_eq!(nav.current(), Route::Detail);
    assert_eq!(nav.depth(), 1);
}

#[test]
fn back_closes_an_open_overlay_before_popping_a_page() {
    let nav = Navigator::new(Route::Home);
    nav.push(Route::Settings);
    let closed = std::rc::Rc::new(std::cell::Cell::new(false));
    let id = {
        let closed = closed.clone();
        ui_core::dismiss::register_dismiss(std::rc::Rc::new(move || closed.set(true)))
    };

    assert!(nav.back(), "the open overlay consumed the back");
    assert!(closed.get(), "the overlay was dismissed");
    assert_eq!(
        nav.current(),
        Route::Settings,
        "the page stack was left alone while a dialog was up"
    );

    assert!(nav.back(), "with nothing open, back pops the page");
    assert_eq!(nav.current(), Route::Home);

    assert!(
        !nav.back(),
        "at the root with nothing open, back is unhandled so the OS gesture can take it"
    );
    ui_core::dismiss::unregister_dismiss(id);
}

#[test]
fn from_signal_adopts_an_existing_stack() {
    let stack = signal(vec![Route::Home, Route::Settings]);
    let nav = Navigator::from_signal(stack, Route::Home);
    assert_eq!(nav.current(), Route::Settings, "the restored top is kept");
    assert_eq!(nav.depth(), 2);
    nav.push(Route::Detail);
    assert_eq!(
        stack.with(|s| s.len()),
        3,
        "the navigator drives the adopted signal"
    );
}

#[test]
fn from_signal_seeds_an_empty_stack_with_the_root() {
    let nav = Navigator::from_signal(signal(Vec::new()), Route::Home);
    assert_eq!(nav.current(), Route::Home);
    assert_eq!(nav.depth(), 1);
    assert!(!nav.can_pop(), "a signal-seeded stack starts at its root");
}

#[test]
fn current_is_reactive() {
    let nav = Navigator::new(Route::Home);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Route>::new()));
    let s = seen.clone();
    let n = nav.clone();
    let _e = reactive_core::effect(move || s.borrow_mut().push(n.current()));
    nav.push(Route::Settings);
    nav.pop();
    assert_eq!(
        *seen.borrow(),
        vec![Route::Home, Route::Settings, Route::Home],
        "the effect re-ran on each navigation"
    );
}
