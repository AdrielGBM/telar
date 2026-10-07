use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Route {
    Home,
    Settings,
    Detail,
}

impl crate::Route for Route {
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

    fn pages() -> Vec<Self> {
        vec![Route::Home, Route::Settings]
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
    let dialog = {
        let closed = closed.clone();
        telar::DismissRegistration::new(std::rc::Rc::new(move || closed.set(true)))
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
    drop(dialog);
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
    let _e = telar::effect(move || s.borrow_mut().push(n.current()));
    nav.push(Route::Settings);
    nav.pop();
    assert_eq!(
        *seen.borrow(),
        vec![Route::Home, Route::Settings, Route::Home],
        "the effect re-ran on each navigation"
    );
}

fn at(path: &str) -> Location {
    telar::LocationFormat::root().parse(path).unwrap()
}

fn steps() -> Vec<(telar::HistoryStep, usize)> {
    telar::take_window_commands()
        .into_iter()
        .filter_map(|command| match command {
            telar::WindowCommand::Navigate(update) => Some((update.step, update.history.len())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_following_navigator_opens_on_the_address_it_was_launched_at() {
    telar::receive_location_history(vec![at("/"), at("/settings")]);
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
    use telar::HistoryStep;
    telar::receive_location_history(vec![at("/")]);
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
    assert_eq!(telar::location_history(), [at("/detail")]);
}

#[test]
fn a_platform_that_names_nothing_is_given_the_current_page() {
    let nav = Navigator::new(Route::Home).follow_location();
    assert_eq!(nav.current(), Route::Home);
    assert_eq!(steps(), [(telar::HistoryStep::Replace, 1)]);
}

#[test]
fn the_platform_moving_by_itself_moves_the_stack_without_an_echo() {
    telar::receive_location_history(vec![at("/")]);
    let nav = Navigator::new(Route::Home).follow_location();
    nav.push(Route::Settings);
    nav.push(Route::Detail);
    steps();

    telar::receive_location_history(vec![at("/"), at("/settings")]);
    assert_eq!(nav.current(), Route::Settings);
    telar::receive_location_history(vec![at("/"), at("/settings"), at("/detail")]);
    assert_eq!(nav.depth(), 3);
    assert!(
        steps().is_empty(),
        "back and forward are already the platform's own"
    );
}

#[test]
fn an_unknown_entry_is_dropped_and_the_current_entry_rewritten_rather_than_stepped_back() {
    telar::receive_location_history(vec![at("/"), at("/settings"), at("/gone")]);
    let nav = Navigator::new(Route::Home).follow_location();
    assert_eq!(
        nav.peek_stack(<[Route]>::to_vec),
        [Route::Home, Route::Settings]
    );
    assert_eq!(steps(), [(telar::HistoryStep::Replace, 2)]);

    telar::receive_location_history(vec![at("/nowhere")]);
    assert_eq!(
        nav.depth(),
        2,
        "nothing recognised leaves the stack where it was"
    );
    assert_eq!(steps(), [(telar::HistoryStep::Replace, 2)]);
}

#[test]
fn locations_asked_for_by_address_open_through_the_following_navigator() {
    telar::receive_location_history(vec![at("/")]);
    let nav = Navigator::new(Route::Home).follow_location();
    assert!(telar::push_location(at("/settings")));
    assert!(!telar::push_location(at("/gone")), "no page, no entry");
    assert!(telar::replace_location(at("/detail")));
    assert_eq!(
        nav.peek_stack(<[Route]>::to_vec),
        [Route::Home, Route::Detail]
    );
    assert!(telar::history_back());
    assert!(
        !telar::history_back(),
        "the root is the platform's to leave"
    );
    assert_eq!(nav.current(), Route::Home);
}

#[test]
fn a_binding_made_under_an_owner_ends_with_it() {
    telar::receive_location_history(vec![at("/")]);
    let owner = {
        let scope = telar::owner_scope();
        Navigator::new(Route::Home).follow_location();
        scope.id()
    };
    telar::dispose_owner(owner);
    assert!(telar::push_location(at("/settings")));
    assert_eq!(
        telar::location_history(),
        [at("/"), at("/settings")],
        "with nothing following, the request moves the history itself"
    );
}

#[test]
fn an_anchor_on_a_page_is_that_page_and_survives_the_next_one() {
    use telar::HistoryStep;
    telar::receive_location_history(vec![at("/"), at("/settings#privacy")]);
    let nav = Navigator::new(Route::Home).follow_location();
    assert_eq!(
        nav.peek_stack(<[Route]>::to_vec),
        [Route::Home, Route::Settings]
    );
    assert!(steps().is_empty(), "the anchor is not rewritten away");
    assert!(telar::push_location(at("/detail#top")));
    assert_eq!(nav.current(), Route::Detail);
    assert_eq!(
        telar::location_history(),
        [
            at("/"),
            at("/settings#privacy"),
            at("/detail"),
            at("/detail#top")
        ]
    );
    assert!(nav.pop());
    assert_eq!(
        steps().last(),
        Some(&(HistoryStep::Back(2), 2)),
        "back from the next page returns to the anchor the reader left"
    );
}

thread_local! {
    static LANGUAGE: telar::RwSignal<&'static str> =
        telar::detached(|| telar::signal("en"));
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    Home,
    Credits,
}

impl crate::Route for Page {
    fn to_location(&self) -> Location {
        match self {
            Page::Home => Location::root(),
            Page::Credits => Location::root().segment("credits"),
        }
    }

    fn from_location(location: &Location) -> Option<Self> {
        match location.segments() {
            [] => Some(Page::Home),
            [s] if s == "credits" => Some(Page::Credits),
            _ => None,
        }
    }

    fn title(&self) -> Option<String> {
        let spanish = LANGUAGE.with(|language| language.get()) == "es";
        match self {
            Page::Home => None,
            Page::Credits if spanish => Some("Créditos".to_owned()),
            Page::Credits => Some("Credits".to_owned()),
        }
    }
}

#[test]
fn the_followed_route_names_the_page_in_the_surface_title() {
    telar::open_surface_title("Portfolio", "Portfolio");
    telar::receive_location_history(vec![at("/")]);
    let nav = Navigator::new(Page::Home).follow_location();
    assert_eq!(telar::surface_title(), "Portfolio");
    nav.push(Page::Credits);
    assert_eq!(telar::surface_title(), "Credits — Portfolio");
    nav.pop();
    assert_eq!(telar::surface_title(), "Portfolio");
}

#[test]
fn the_page_title_follows_what_the_route_reads_for_it() {
    telar::open_surface_title("Portfolio", "Portfolio");
    telar::receive_location_history(vec![at("/"), at("/credits")]);
    Navigator::new(Page::Home).follow_location();
    assert_eq!(telar::surface_title(), "Credits — Portfolio");
    LANGUAGE.with(|language| language.set("es"));
    assert_eq!(telar::surface_title(), "Créditos — Portfolio");
}

#[test]
fn a_navigator_no_longer_followed_no_longer_names_the_page() {
    telar::open_surface_title("Portfolio", "Portfolio");
    telar::receive_location_history(vec![at("/")]);
    let replaced = Navigator::new(Page::Home).follow_location();
    let _following = Navigator::new(Route::Home).follow_location();
    replaced.push(Page::Credits);
    assert_eq!(telar::surface_title(), "Portfolio");
}

#[test]
fn a_binding_that_ends_takes_its_page_title_with_it() {
    telar::open_surface_title("Portfolio", "Portfolio");
    telar::receive_location_history(vec![at("/"), at("/credits")]);
    let owner = {
        let scope = telar::owner_scope();
        Navigator::new(Page::Home).follow_location();
        scope.id()
    };
    assert_eq!(telar::surface_title(), "Credits — Portfolio");
    telar::dispose_owner(owner);
    assert_eq!(telar::surface_title(), "Portfolio");
}

#[test]
fn the_following_navigator_declares_its_route_types_pages() {
    telar::receive_location_history(vec![at("/")]);
    let _nav = Navigator::new(Route::Home).follow_location();
    assert_eq!(telar::location_pages(), [at("/"), at("/settings")]);
}

#[test]
fn a_route_type_that_names_no_pages_declares_none() {
    telar::receive_location_history(vec![at("/")]);
    let _nav = Navigator::new(Page::Home).follow_location();
    assert!(telar::location_pages().is_empty());
}
