use platform_core::ModifiersState;

use super::*;

#[test]
fn shift_is_coarse_alt_is_fine_and_both_cancel() {
    let held = |is_shift, is_alt| ModifiersState {
        is_shift,
        is_alt,
        ..Default::default()
    };
    assert_eq!(step_factor(held(false, false)), 1.0);
    assert_eq!(step_factor(held(true, false)), 10.0);
    assert_eq!(step_factor(held(false, true)), 0.1);
    assert_eq!(step_factor(held(true, true)), 1.0);
}
