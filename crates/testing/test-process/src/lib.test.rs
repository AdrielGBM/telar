use std::panic::catch_unwind;

use super::*;

const GIVEN_VAR: &str = "TELAR_TEST_PROCESS_GIVEN";
const FAILING: &str = "fails_in_its_own_process";

#[test]
fn a_test_runs_in_its_own_process_with_the_environment_it_was_given() {
    let mut own = OwnProcess::new(
        module_path!(),
        "a_test_runs_in_its_own_process_with_the_environment_it_was_given",
    );
    own.env(GIVEN_VAR, "given");
    own.run(|| assert_eq!(std::env::var(GIVEN_VAR).as_deref(), Ok("given")));
}

/// Not a test of its own: the process [`a_failure_in_its_own_process_is_reported_with_what_it_printed`] starts, which fails.
#[test]
fn fails_in_its_own_process() {
    if OwnProcess::new(module_path!(), FAILING).is_current() {
        println!("printed before failing");
        panic!("failed on purpose");
    }
}

fn failure_of(own: OwnProcess) -> String {
    let failure = catch_unwind(|| own.passes()).expect_err("the test did not pass");
    failure
        .downcast::<String>()
        .map(|message| *message)
        .expect("a formatted message")
}

#[test]
fn a_failure_in_its_own_process_is_reported_with_what_it_printed() {
    let message = failure_of(OwnProcess::new(module_path!(), FAILING));
    assert!(message.contains("printed before failing"), "{message}");
    assert!(message.contains("failed on purpose"), "{message}");
}

#[test]
fn a_name_that_runs_no_test_does_not_pass() {
    let message = failure_of(OwnProcess::new(module_path!(), "no_such_test"));
    assert!(message.contains("0 passed"), "{message}");
}

#[test]
fn a_test_at_the_root_of_its_binary_is_named_without_a_module() {
    let own = OwnProcess::new("binary", "a_test");
    let args: Vec<_> = own.command().get_args().map(ToOwned::to_owned).collect();
    assert_eq!(args[0], "a_test");
    let nested = OwnProcess::new("binary::module::tests", "a_test");
    let args: Vec<_> = nested.command().get_args().map(ToOwned::to_owned).collect();
    assert_eq!(args[0], "module::tests::a_test");
}
