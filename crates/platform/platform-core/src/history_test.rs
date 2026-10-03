use std::cell::RefCell;

use super::*;
use crate::{HistoryStep, LocationFormat, take_window_commands};

fn at(path: &str) -> Location {
    LocationFormat::root().parse(path).unwrap()
}

fn navigations() -> Vec<HistoryUpdate> {
    take_window_commands()
        .into_iter()
        .filter_map(|command| match command {
            WindowCommand::Navigate(update) => Some(update),
            _ => None,
        })
        .collect()
}

#[derive(Default)]
struct Recorder {
    calls: RefCell<Vec<String>>,
}

impl HistoryFollower for Recorder {
    fn adopt(&self, history: &[Location]) {
        let paths: Vec<String> = history
            .iter()
            .map(|l| LocationFormat::root().format(l))
            .collect();
        self.calls
            .borrow_mut()
            .push(format!("adopt {}", paths.join(" ")));
    }

    fn push(&self, location: &Location) -> bool {
        self.calls
            .borrow_mut()
            .push(format!("push {}", LocationFormat::root().format(location)));
        true
    }

    fn replace(&self, location: &Location) -> bool {
        self.calls.borrow_mut().push(format!(
            "replace {}",
            LocationFormat::root().format(location)
        ));
        true
    }

    fn back(&self) -> bool {
        self.calls.borrow_mut().push("back".into());
        false
    }
}

#[test]
fn a_reported_history_reaches_the_platform_as_one_step() {
    receive_location_history(vec![at("/")]);
    report_location_history(vec![at("/"), at("/a")]);
    assert_eq!(
        navigations(),
        [HistoryUpdate {
            step: HistoryStep::Push(1),
            history: vec![at("/"), at("/a")],
        }]
    );
    assert_eq!(location_history(), [at("/"), at("/a")]);

    report_location_history(vec![at("/"), at("/a")]);
    assert!(
        navigations().is_empty(),
        "a history the platform already has is not sent again"
    );
}

#[test]
fn a_received_history_is_never_echoed_back() {
    receive_location_history(vec![at("/"), at("/a"), at("/b")]);
    report_location_history(vec![at("/"), at("/a"), at("/b")]);
    assert!(navigations().is_empty());
}

#[test]
fn a_rewrite_replaces_the_current_entry_even_where_a_diff_would_step_back() {
    receive_location_history(vec![at("/"), at("/unknown")]);
    rewrite_location_history(vec![at("/")]);
    assert_eq!(
        navigations(),
        [HistoryUpdate {
            step: HistoryStep::Replace,
            history: vec![at("/")],
        }]
    );
    rewrite_location_history(vec![at("/")]);
    assert!(navigations().is_empty());
}

#[test]
fn the_follower_hears_every_received_history_and_carries_every_request() {
    let recorder = Rc::new(Recorder::default());
    let id = follow_location_history(recorder.clone());
    receive_location_history(vec![at("/"), at("/a")]);
    assert!(push_location(at("/b")));
    assert!(replace_location(at("/c")));
    assert!(!history_back(), "the follower's own answer is the answer");
    assert_eq!(
        *recorder.calls.borrow(),
        ["adopt / /a", "push /b", "replace /c", "back"]
    );
    assert!(
        navigations().is_empty(),
        "the follower reports its own moves"
    );

    unfollow_location_history(id);
    receive_location_history(vec![at("/")]);
    assert_eq!(recorder.calls.borrow().len(), 4);
}

#[test]
fn a_replaced_follower_cannot_withdraw_its_successor() {
    let first = Rc::new(Recorder::default());
    let second = Rc::new(Recorder::default());
    let first_id = follow_location_history(first.clone());
    follow_location_history(second.clone());
    unfollow_location_history(first_id);
    receive_location_history(vec![at("/")]);
    assert!(first.calls.borrow().is_empty());
    assert_eq!(*second.calls.borrow(), ["adopt /"]);
}

#[test]
fn without_a_follower_requests_move_the_history_itself() {
    receive_location_history(vec![at("/")]);
    assert!(push_location(at("/a")));
    assert!(replace_location(at("/b")));
    assert!(history_back());
    assert!(
        !history_back(),
        "the first entry is the platform's to leave"
    );
    let steps: Vec<HistoryStep> = navigations().into_iter().map(|u| u.step).collect();
    assert_eq!(
        steps,
        [
            HistoryStep::Push(1),
            HistoryStep::Replace,
            HistoryStep::Back(1)
        ]
    );
    assert_eq!(location_history(), [at("/")]);
}

fn revealed() -> Rc<RefCell<Vec<String>>> {
    let revealed = Rc::new(RefCell::new(Vec::new()));
    let sink = revealed.clone();
    set_anchor_revealer(move |anchor| sink.borrow_mut().push(anchor.to_owned()));
    revealed
}

#[test]
fn a_followed_anchor_is_an_entry_of_its_own_on_the_same_page() {
    let revealed = revealed();
    let recorder = Rc::new(Recorder::default());
    let id = follow_location_history(recorder.clone());
    receive_location_history(vec![at("/"), at("/a")]);
    navigations();
    push_anchor("contact");
    assert_eq!(location_history(), [at("/"), at("/a"), at("/a#contact")]);
    assert_eq!(
        navigations(),
        [HistoryUpdate {
            step: HistoryStep::Push(1),
            history: vec![at("/"), at("/a"), at("/a#contact")],
        }]
    );
    push_anchor("contact");
    assert!(
        navigations().is_empty(),
        "the same anchor twice is one entry"
    );
    assert_eq!(*revealed.borrow(), ["contact", "contact"]);
    assert_eq!(
        *recorder.calls.borrow(),
        ["adopt / /a"],
        "the follower sees pages, and the page did not change"
    );
    unfollow_location_history(id);
}

#[test]
fn the_follower_sees_the_pages_an_anchored_history_shows() {
    let _revealed = revealed();
    let recorder = Rc::new(Recorder::default());
    let id = follow_location_history(recorder.clone());
    receive_location_history(vec![at("/a#intro"), at("/a#contact"), at("/b")]);
    assert_eq!(*recorder.calls.borrow(), ["adopt /a /b"]);
    assert_eq!(pages_of(&[at("/a"), at("/a")]), [at("/a"), at("/a")]);
    unfollow_location_history(id);
}

#[test]
fn a_reported_page_keeps_the_anchor_entries_of_the_pages_that_stay() {
    receive_location_history(vec![at("/a"), at("/a#contact"), at("/b")]);
    report_location_history(vec![at("/a")]);
    assert_eq!(
        navigations(),
        [HistoryUpdate {
            step: HistoryStep::Back(1),
            history: vec![at("/a"), at("/a#contact")],
        }],
        "back from the next page returns to where the reader was on this one"
    );
    report_location_history(vec![at("/a"), at("/c")]);
    assert_eq!(location_history(), [at("/a"), at("/a#contact"), at("/c")]);
}

#[test]
fn arriving_at_an_anchor_reveals_it_once_something_can() {
    receive_location_history(vec![at("/a#simulation")]);
    let revealed = revealed();
    assert_eq!(*revealed.borrow(), ["simulation"]);
    receive_location_history(vec![at("/a#simulation")]);
    assert_eq!(
        revealed.borrow().len(),
        1,
        "an entry the platform already showed is not arrived at again"
    );
}

#[test]
fn pushing_another_pages_anchor_opens_the_page_then_the_anchor() {
    let revealed = revealed();
    receive_location_history(vec![at("/")]);
    navigations();
    assert!(push_location(at("/b#team")));
    assert_eq!(location_history(), [at("/"), at("/b"), at("/b#team")]);
    assert_eq!(*revealed.borrow(), ["team"]);
}

#[test]
fn back_over_an_anchor_leaves_the_page_where_it_is() {
    let recorder = Rc::new(Recorder::default());
    let id = follow_location_history(recorder.clone());
    receive_location_history(vec![at("/a"), at("/a#contact")]);
    assert!(history_back());
    assert_eq!(location_history(), [at("/a")]);
    assert_eq!(*recorder.calls.borrow(), ["adopt /a"]);
    unfollow_location_history(id);
}

fn in_locales(path: &str) -> Location {
    LocationFormat::root()
        .with_locales(["es", "en"])
        .parse(path)
        .unwrap()
}

#[derive(Default)]
struct Language {
    chooses: String,
    adopted: RefCell<Vec<String>>,
}

impl LocaleFollower for Language {
    fn choose(&self) -> String {
        self.chooses.clone()
    }

    fn adopt(&self, locale: &str) {
        self.adopted.borrow_mut().push(locale.to_owned());
    }
}

fn bind(chooses: &str) -> Rc<Language> {
    let language = Rc::new(Language {
        chooses: chooses.to_owned(),
        ..Language::default()
    });
    bind_location_locale(vec!["es".into(), "en".into()], language.clone());
    language
}

#[test]
fn the_address_opened_at_decides_the_locale_over_one_set_before() {
    let language = bind("es");
    set_location_locale("es");
    receive_location_history(vec![in_locales("/en/a")]);
    assert_eq!(location_locale().as_deref(), Some("en"));
    assert_eq!(*language.adopted.borrow(), ["en"]);
    assert!(navigations().is_empty());
}

#[test]
fn an_address_naming_no_locale_is_written_in_the_one_chosen() {
    let language = bind("en");
    receive_location_history(vec![at("/"), at("/a")]);
    assert_eq!(*language.adopted.borrow(), ["en"]);
    assert_eq!(
        navigations(),
        [HistoryUpdate {
            step: HistoryStep::Replace,
            history: vec![in_locales("/en/"), in_locales("/en/a")],
        }]
    );
}

#[test]
fn the_follower_never_sees_a_locale() {
    let _language = bind("es");
    let recorder = Rc::new(Recorder::default());
    let id = follow_location_history(recorder.clone());
    receive_location_history(vec![in_locales("/es/"), in_locales("/es/a#team")]);
    assert_eq!(*recorder.calls.borrow(), ["adopt / /a"]);
    report_location_history(vec![at("/"), at("/a"), at("/b")]);
    assert_eq!(
        location_history(),
        [
            in_locales("/es/"),
            in_locales("/es/a#team"),
            in_locales("/es/b")
        ],
        "a page the follower adds is written in the locale"
    );
    unfollow_location_history(id);
}

#[test]
fn a_switch_rewrites_every_entry_in_place() {
    let language = bind("es");
    receive_location_history(vec![in_locales("/es/"), in_locales("/es/#contact")]);
    navigations();
    set_location_locale("en");
    assert_eq!(
        navigations(),
        [HistoryUpdate {
            step: HistoryStep::Replace,
            history: vec![in_locales("/en/"), in_locales("/en/#contact")],
        }]
    );
    set_location_locale("fr");
    assert!(
        navigations().is_empty(),
        "a locale the address does not carry"
    );
    assert_eq!(
        *language.adopted.borrow(),
        ["es"],
        "the app asked for the switch, so it is not told of it"
    );
}

#[test]
fn an_app_that_binds_no_locale_keeps_its_first_segments() {
    receive_location_history(vec![at("/es/a")]);
    assert_eq!(location_history(), [Location::from_segments(["es", "a"])]);
    assert_eq!(location_locale(), None);
    set_location_locale("en");
    assert!(navigations().is_empty());
}

#[test]
fn a_locale_bound_after_the_history_opened_settles_on_what_it_shows() {
    receive_location_history(vec![at("/a")]);
    navigations();
    let language = bind("en");
    assert_eq!(*language.adopted.borrow(), ["en"]);
    assert_eq!(location_history(), [in_locales("/en/a")]);
    assert_eq!(navigations().len(), 1);
}

#[test]
fn a_renamed_anchor_is_renamed_in_every_entry_naming_it() {
    receive_location_history(vec![at("/a#simulacion"), at("/b")]);
    navigations();
    rename_anchor("simulacion", "simulation");
    assert_eq!(location_history(), [at("/a#simulation"), at("/b")]);
    assert_eq!(
        navigations(),
        [HistoryUpdate {
            step: HistoryStep::Replace,
            history: vec![at("/a#simulation"), at("/b")],
        }]
    );
    rename_anchor("elsewhere", "anywhere");
    assert!(navigations().is_empty());
}
