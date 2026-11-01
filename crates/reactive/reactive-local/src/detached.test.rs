use super::*;

#[test]
fn a_panic_inside_leaves_the_depth_where_it_found_it() {
    let outcome = std::panic::catch_unwind(|| {
        detached(|| {
            detached(|| panic!("boom"));
        })
    });
    assert!(
        outcome.is_err(),
        "the panic is reported rather than swallowed"
    );
    assert!(
        !is_detached(),
        "an unwind must not leave the thread detached"
    );
}
