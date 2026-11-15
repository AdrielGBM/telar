use super::*;

#[test]
fn a_range_that_runs_backwards_is_refused() {
    assert_eq!(
        NumericValue::new(5.0..=1.0, 3.0),
        Err(NumericValueError::Range { min: 5.0, max: 1.0 })
    );
}

#[test]
fn a_reading_outside_its_range_is_refused() {
    assert_eq!(
        NumericValue::new(0.0..=1.0, 1.5),
        Err(NumericValueError::Outside {
            now: 1.5,
            min: 0.0,
            max: 1.0
        })
    );
    assert!(NumericValue::new(0.0..=1.0, f64::NAN).is_err());
    assert!(NumericValue::new(f64::NAN..=1.0, 0.5).is_err());
}

#[test]
fn an_unbounded_range_holds_any_number() {
    let reading = NumericValue::new(f64::NEG_INFINITY..=f64::INFINITY, -40.0)
        .expect("an unbounded range holds every number");
    assert_eq!(
        (reading.now(), reading.min(), reading.max()),
        (-40.0, f64::NEG_INFINITY, f64::INFINITY)
    );
}
