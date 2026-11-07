use super::*;

const OVER_BUDGET: ErrorCode = ErrorCode::TooMuchMemory {
    budget: MEMORY_BUDGET,
};

#[test]
fn a_refused_charge_takes_nothing() {
    let budget = Budget::new();
    assert_eq!(
        budget.charge(MEMORY_BUDGET + 1).unwrap_err().code,
        OVER_BUDGET
    );
    assert_eq!(budget.charge(MEMORY_BUDGET), Ok(()));
    assert_eq!(budget.charge(1).unwrap_err().code, OVER_BUDGET);
}

#[test]
fn a_writer_refuses_the_piece_that_would_overspend() {
    let budget = Budget::new();
    budget.charge(MEMORY_BUDGET - 4).unwrap();
    let mut out = budget.writer();
    assert_eq!(out.push_str("abc"), Ok(()));
    assert_eq!(out.push_str("de").unwrap_err().code, OVER_BUDGET);
    assert_eq!(out.push('d'), Ok(()));
    assert_eq!(
        out.push_value(&Value::list([Value::text("x"), Value::text("y")]))
            .unwrap_err()
            .code,
        OVER_BUDGET
    );
    assert_eq!(out.finish(), Value::text("abcd"));
}
