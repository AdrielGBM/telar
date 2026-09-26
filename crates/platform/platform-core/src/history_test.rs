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
