use std::cell::Cell;
use std::rc::Rc;

use reactive_core::effect;

use super::*;

fn args(text: &str) -> Vec<String> {
    vec![text.to_string()]
}

#[test]
fn a_call_is_kept_with_its_name_and_arguments() {
    let log = ActionLog::new();
    log.log("on_press", Vec::new());
    log.log("on_change", args("\"hi\""));
    assert_eq!(
        log.calls(),
        [
            ActionCall::new(1, "on_press", Vec::new()),
            ActionCall::new(2, "on_change", args("\"hi\"")),
        ]
    );
    assert_eq!(log.last().map(|call| call.name), Some("on_change"));
    assert_eq!(log.count_of("on_press"), 1);
}

#[test]
fn a_full_log_drops_its_oldest_call_and_still_counts_it() {
    let log = ActionLog::with_capacity(2);
    for n in 0..3 {
        log.log("on_step", args(&n.to_string()));
    }
    let seqs: Vec<u64> = log.calls().iter().map(|call| call.seq).collect();
    assert_eq!(seqs, [2, 3]);
    assert_eq!(log.count(), 3);
}

#[test]
fn clearing_empties_the_log_without_reusing_a_sequence_number() {
    let log = ActionLog::new();
    log.log("on_press", Vec::new());
    log.clear();
    assert!(log.calls().is_empty());
    assert_eq!(log.count(), 0);
    log.log("on_press", Vec::new());
    assert_eq!(log.last().map(|call| call.seq), Some(2));
}

#[test]
fn a_reader_hears_each_call() {
    let log = ActionLog::new();
    let heard = Rc::new(Cell::new(0));
    effect({
        let heard = Rc::clone(&heard);
        move || heard.set(log.count())
    });
    log.log("on_press", Vec::new());
    log.log("on_press", Vec::new());
    assert_eq!(heard.get(), 2);
}

#[test]
fn a_log_keeps_at_least_one_call() {
    let log = ActionLog::with_capacity(0);
    log.log("on_press", Vec::new());
    assert_eq!(log.capacity(), 1);
    assert_eq!(log.calls().len(), 1);
}

fn texts(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn logged_calls(log: &ActionLog) -> Vec<(&'static str, Vec<String>)> {
    log.calls()
        .into_iter()
        .map(|call| (call.name, call.args))
        .collect()
}

#[test]
fn an_action_records_each_call_by_its_arguments_debug_text() {
    let log = ActionLog::new();
    <Handler>::action(log, "on_press").run();
    <Handler<fn(u32)>>::action(log, "on_select").run(2);
    <Handler<fn(f32, bool)>>::action(log, "on_end").run(0.5, true);
    <Handler<fn(&'static str, char, Option<u8>)>>::action(log, "on_edit").run("hi", 'x', None);
    assert_eq!(
        logged_calls(&log),
        [
            ("on_press", Vec::new()),
            ("on_select", texts(&["2"])),
            ("on_end", texts(&["0.5", "true"])),
            ("on_edit", texts(&["\"hi\"", "'x'", "None"])),
        ]
    );
}

#[test]
fn a_logged_callback_records_the_call_then_runs() {
    let log = ActionLog::new();
    let seen = Rc::new(Cell::new(0));
    let callback: Handler<fn(u32)> = Handler::from({
        let seen = Rc::clone(&seen);
        move |value| {
            assert_eq!(
                log.count(),
                1,
                "the call is recorded before the callback runs"
            );
            seen.set(value);
        }
    });
    callback.logged(log, "on_change").run(7);
    assert_eq!(seen.get(), 7);
    assert_eq!(logged_calls(&log), [("on_change", texts(&["7"]))]);
}

#[test]
fn an_unset_optional_callback_stays_unset_and_a_set_one_keeps_running() {
    let log = ActionLog::new();
    let ran = Rc::new(Cell::new(false));
    let set: Option<Handler> = Some(Handler::from({
        let ran = Rc::clone(&ran);
        move || ran.set(true)
    }));
    let unset: Option<Handler<fn(bool)>> = None;
    set.logged(log, "on_close").expect("still set").run();
    assert!(
        unset.logged(log, "on_toggle").is_none(),
        "a component that draws something for a callback must not be handed one it was not given"
    );
    assert!(ran.get());
    assert_eq!(logged_calls(&log), [("on_close", Vec::new())]);
}

struct Node;

#[test]
fn the_probe_logs_a_callback_whose_arguments_have_no_debug_by_their_type() {
    let log = ActionLog::new();
    let callback: Option<Handler<fn(Node, u8)>> = Some(Handler::from(|_, _| {}));
    let callback = crate::__preview_action!(log, "on_pick", callback);
    callback.expect("still set").run(Node, 3);
    let call = log.last().expect("logged");
    assert_eq!(call.name, "on_pick");
    assert_eq!(call.args.len(), 2);
    assert!(call.args[0].ends_with("Node>"), "{:?}", call.args);
    assert!(call.args[1].ends_with("u8>"), "{:?}", call.args);
}

#[test]
fn the_probe_leaves_anything_but_a_callback_as_it_was() {
    let log = ActionLog::new();
    let label = crate::__preview_action!(log, "label", String::from("Save"));
    let style: Rc<dyn Fn(u32) -> u32> = Rc::new(|value| value + 1);
    let style = crate::__preview_action!(log, "style", style);
    let unset: Option<Rc<dyn Fn() -> bool>> = None;
    let unset = crate::__preview_action!(log, "enabled", unset);
    assert_eq!(label, "Save");
    assert_eq!(style(1), 2);
    assert!(
        unset.is_none(),
        "a callback that returns a value is not an action"
    );
    assert_eq!(log.count(), 0);
}

#[test]
fn a_callback_that_outlives_its_log_records_nothing() {
    let scope = reactive_core::owner_scope();
    let owner = scope.id();
    let log = ActionLog::new();
    drop(scope);
    let action = <Handler>::action(log, "on_press");
    reactive_core::dispose_owner(owner);
    action.run();
}

#[test]
fn the_probe_logs_a_handler_before_it_runs() {
    let log = ActionLog::new();
    let ran = Rc::new(Cell::new(false));
    let handler = Handler::from({
        let ran = Rc::clone(&ran);
        move || {
            assert_eq!(log.count(), 1, "the press is recorded before it runs");
            ran.set(true);
        }
    });
    let unset: Option<Handler> = None;
    let handler = crate::__preview_action!(log, "on_press", handler);
    let unset = crate::__preview_action!(log, "on_close", unset);
    handler.run();
    assert!(ran.get());
    assert!(unset.is_none());
    assert_eq!(logged_calls(&log), [("on_press", Vec::new())]);
}
