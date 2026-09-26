use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Route {
    Home,
    Settings,
    Detail,
}

impl crate::route::Route for Route {
    fn to_location(&self) -> Location {
        match self {
            Route::Home => Location::root(),
            Route::Settings => Location::root().segment("settings"),
            Route::Detail => Location::root().segment("detail"),
        }
    }

    fn from_location(location: &Location) -> Option<Self> {
        match location.segments() {
            [] => Some(Route::Home),
            [s] if s == "settings" => Some(Route::Settings),
            [s] if s == "detail" => Some(Route::Detail),
            _ => None,
        }
    }
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
fn location_reads_the_current_page_as_a_location() {
    let nav = Navigator::new(Route::Home);
    assert_eq!(nav.location(), Location::root());
    nav.push(Route::Settings);
    assert_eq!(nav.location(), Location::root().segment("settings"));
}

#[test]
fn locations_reads_the_whole_stack_root_first() {
    let nav = Navigator::new(Route::Home);
    nav.push(Route::Settings);
    nav.push(Route::Detail);
    assert_eq!(
        nav.locations(),
        vec![
            Location::root(),
            Location::root().segment("settings"),
            Location::root().segment("detail"),
        ]
    );
}

#[test]
fn current_is_reactive() {
    let nav = Navigator::new(Route::Home);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Route>::new()));
    let s = seen.clone();
    let n = nav;
    let _e = reactive_core::effect(move || s.borrow_mut().push(n.current()));
    nav.push(Route::Settings);
    nav.pop();
    assert_eq!(
        *seen.borrow(),
        vec![Route::Home, Route::Settings, Route::Home],
        "the effect re-ran on each navigation"
    );
}

fn at(path: &str) -> Location {
    platform_core::LocationFormat::root().parse(path).unwrap()
}

fn steps() -> Vec<(platform_core::HistoryStep, usize)> {
    platform_core::take_window_commands()
        .into_iter()
        .filter_map(|command| match command {
            platform_core::WindowCommand::Navigate(update) => {
                Some((update.step, update.history.len()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_following_navigator_opens_on_the_address_it_was_launched_at() {
    platform_core::receive_location_history(vec![at("/"), at("/settings")]);
    let nav = Navigator::new(Route::Home).follow_location();
    assert_eq!(
        nav.peek_stack(<[Route]>::to_vec),
        [Route::Home, Route::Settings]
    );
    assert!(
        steps().is_empty(),
        "the platform already shows this history"
    );
}

#[test]
fn every_move_of_the_stack_reaches_the_platform_as_one_step() {
    use platform_core::HistoryStep;
    platform_core::receive_location_history(vec![at("/")]);
    let nav = Navigator::new(Route::Home).follow_location();
    nav.push(Route::Settings);
    nav.push(Route::Detail);
    nav.replace(Route::Settings);
    nav.pop_to_root();
    nav.reset(Route::Detail);
    assert_eq!(
        steps(),
        [
            (HistoryStep::Push(1), 2),
            (HistoryStep::Push(1), 3),
            (HistoryStep::Replace, 3),
            (HistoryStep::Back(2), 1),
            (HistoryStep::Replace, 1),
        ]
    );
    assert_eq!(platform_core::location_history(), [at("/detail")]);
}

#[test]
fn a_platform_that_names_nothing_is_given_the_current_page() {
    let nav = Navigator::new(Route::Home).follow_location();
    assert_eq!(nav.current(), Route::Home);
    assert_eq!(steps(), [(platform_core::HistoryStep::Replace, 1)]);
}

#[test]
fn the_platform_moving_by_itself_moves_the_stack_without_an_echo() {
    platform_core::receive_location_history(vec![at("/")]);
    let nav = Navigator::new(Route::Home).follow_location();
    nav.push(Route::Settings);
    nav.push(Route::Detail);
    steps();

    platform_core::receive_location_history(vec![at("/"), at("/settings")]);
    assert_eq!(nav.current(), Route::Settings);
    platform_core::receive_location_history(vec![at("/"), at("/settings"), at("/detail")]);
    assert_eq!(nav.depth(), 3);
    assert!(
        steps().is_empty(),
        "back and forward are already the platform's own"
    );
}

#[test]
fn an_unknown_entry_is_dropped_and_the_current_entry_rewritten_rather_than_stepped_back() {
    platform_core::receive_location_history(vec![at("/"), at("/settings"), at("/gone")]);
    let nav = Navigator::new(Route::Home).follow_location();
    assert_eq!(
        nav.peek_stack(<[Route]>::to_vec),
        [Route::Home, Route::Settings]
    );
    assert_eq!(steps(), [(platform_core::HistoryStep::Replace, 2)]);

    platform_core::receive_location_history(vec![at("/nowhere")]);
    assert_eq!(
        nav.depth(),
        2,
        "nothing recognised leaves the stack where it was"
    );
    assert_eq!(steps(), [(platform_core::HistoryStep::Replace, 2)]);
}

#[test]
fn locations_asked_for_by_address_open_through_the_following_navigator() {
    platform_core::receive_location_history(vec![at("/")]);
    let nav = Navigator::new(Route::Home).follow_location();
    assert!(platform_core::push_location(at("/settings")));
    assert!(
        !platform_core::push_location(at("/gone")),
        "no page, no entry"
    );
    assert!(platform_core::replace_location(at("/detail")));
    assert_eq!(
        nav.peek_stack(<[Route]>::to_vec),
        [Route::Home, Route::Detail]
    );
    assert!(platform_core::history_back());
    assert!(
        !platform_core::history_back(),
        "the root is the platform's to leave"
    );
    assert_eq!(nav.current(), Route::Home);
}

#[test]
fn a_binding_made_under_an_owner_ends_with_it() {
    platform_core::receive_location_history(vec![at("/")]);
    let owner = {
        let scope = reactive_core::owner_scope();
        Navigator::new(Route::Home).follow_location();
        scope.id()
    };
    reactive_core::dispose_owner(owner);
    assert!(platform_core::push_location(at("/settings")));
    assert_eq!(
        platform_core::location_history(),
        [at("/"), at("/settings")],
        "with nothing following, the request moves the history itself"
    );
}
