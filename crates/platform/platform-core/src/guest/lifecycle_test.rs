use super::*;
use crate::guest::lent::{ANSWER, LentBytes};
use crate::guest::outcome::REPORT;

extern "C" fn installs_nothing(_: HostServices) -> CallOutcome {
    CallOutcome::OK
}

const SERVED: Lifecycle = Lifecycle::serving(installs_nothing);

#[test]
fn a_guest_counts_from_the_epoch_its_host_hands_it() {
    let host = Epoch::start();
    let outcome = unsafe { (SERVED.adopt_epoch)(host.now()) };
    assert_eq!(unsafe { outcome.into_result() }, Ok(()));
    let instant = host.instant(Timestamp { nanos: 5_000_000 });
    let drift = (host_epoch().stamp(instant).nanos - 5_000_000).abs();
    assert!(drift < 50_000_000, "off by {drift}ns");
}

/// Its last call frees what every entry before it lent this thread, which a guest that panicked is never told to release.
#[test]
fn quiesce_frees_what_the_entries_lent_this_thread() {
    LentBytes::answer(&String::from("an answer"));
    let _ = guest_entry(|| panic!("a press handler panicked"));
    assert!(REPORT.with(|report| report.borrow().is_some()));
    assert!(ANSWER.with(|answer| !answer.borrow().as_slice().is_empty()));

    let outcome = unsafe { (SERVED.quiesce)() };
    assert_eq!(unsafe { outcome.into_result() }, Ok(()));
    assert!(REPORT.with(|report| report.borrow().is_none()));
    assert!(ANSWER.with(|answer| answer.borrow().as_slice().is_empty()));
}
