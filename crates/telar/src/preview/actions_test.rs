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
