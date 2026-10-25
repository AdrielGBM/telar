use super::*;
use crate::guest::Guest;

#[test]
fn an_entry_that_returns_reads_as_ok() {
    let mut ran = false;
    let outcome = guest_entry(|| ran = true);
    assert!(ran);
    assert_eq!(outcome.status(), CallOutcome::RETURNED);
    assert_eq!(unsafe { outcome.into_result() }, Ok(()));
}

#[test]
fn a_panic_crosses_as_its_message_and_where_it_happened() {
    let line = line!() + 1;
    let outcome = guest_entry(|| panic!("the {} broke", "layout"));
    assert_eq!(outcome.status(), CallOutcome::PANICKED);
    let panic = unsafe { outcome.into_result() }.expect_err("the panic is reported");
    assert_eq!(panic.message, "the layout broke");
    let location = panic.location.expect("the hook saw where");
    assert!(
        location.starts_with(&format!("{}:{line}:", file!())),
        "{location}"
    );
}

#[test]
fn a_status_the_contract_does_not_know_is_taken_as_a_panic() {
    let outcome = CallOutcome {
        status: 9,
        ..CallOutcome::OK
    };
    let panic = unsafe { outcome.into_result() }.expect_err("an unknown status is no success");
    assert_eq!(
        panic.message,
        "the guest answered with status 9, which this contract does not know"
    );
    assert_eq!(panic.location, None);
}

#[test]
fn the_host_stops_calling_a_guest_once_an_entry_reports_a_panic() {
    let guest = Guest::default();
    assert_eq!(
        guest.call(|| unsafe { guest_entry(|| {}).into_result() }),
        Some(())
    );
    assert_eq!(
        guest.call(|| unsafe { guest_entry(|| panic!("stop")).into_result() }),
        None
    );
    let mut called = false;
    guest.call(|| unsafe { guest_entry(|| called = true).into_result() });
    assert!(!called);
    assert_eq!(
        guest.panic().map(|panic| panic.message.as_str()),
        Some("stop")
    );
}
