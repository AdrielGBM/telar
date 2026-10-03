//! The launcher calculator's behaviour, which `calculate` replaces: every case of the evaluator it supersedes, plus what that one left unsaid.

use super::*;

fn eval(input: &str) -> Option<String> {
    calculate(input).ok().map(format_number)
}

#[test]
fn arithmetic_follows_precedence_and_brackets() {
    assert_eq!(eval("2+3*4").as_deref(), Some("14"));
    assert_eq!(eval("(2+3)*4").as_deref(), Some("20"));
    assert_eq!(eval("10 / 4").as_deref(), Some("2.5"));
    assert_eq!(eval("-3 + 5").as_deref(), Some("2"));
    assert_eq!(eval("2 * -3").as_deref(), Some("-6"));
    assert_eq!(eval("[1 + 2] * 3").as_deref(), Some("9"));
}

#[test]
fn exponentiation_is_right_associative() {
    assert_eq!(eval("2^10").as_deref(), Some("1024"));
    assert_eq!(
        eval("2^3^2").as_deref(),
        Some("512"),
        "2^(3^2), not (2^3)^2 = 64"
    );
}

#[test]
fn a_leading_minus_binds_looser_than_a_power() {
    assert_eq!(eval("-2^2").as_deref(), Some("-4"));
    assert_eq!(eval("2^-1").as_deref(), Some("0.5"));
}

#[test]
fn percentages_are_relative_to_what_they_are_added_to() {
    assert_eq!(eval("200 + 10%").as_deref(), Some("220"));
    assert_eq!(eval("200 - 10%").as_deref(), Some("180"));
    assert_eq!(eval("50 * 10%").as_deref(), Some("5"));
    assert_eq!(eval("10%").as_deref(), Some("0.1"));
    assert_eq!(eval("200 * (1 + 10%)").as_deref(), Some("220"));
    assert_eq!(
        eval("200 - -10%").as_deref(),
        Some("220"),
        "a negated percentage is still a share of the left side"
    );
}

#[test]
fn functions_and_constants_resolve() {
    assert_eq!(eval("sqrt(16)").as_deref(), Some("4"));
    assert_eq!(eval("round(2.6)").as_deref(), Some("3"));
    assert_eq!(eval("log2(1024)").as_deref(), Some("10"));
    assert_eq!(eval("2 * pi").as_deref(), eval("tau").as_deref());
    assert_eq!(eval("ln(e)").as_deref(), Some("1"));
    assert_eq!(eval("SQRT(9)").as_deref(), Some("3"), "case-insensitive");
    assert_eq!(eval("PI").as_deref(), eval("π").as_deref());
}

#[test]
fn results_hide_binary_float_noise() {
    assert_eq!(eval("0.1 + 0.2").as_deref(), Some("0.3"));
    assert_eq!(eval("1/3").as_deref(), Some("0.3333333333"));
    assert_eq!(format_number(4.0), "4", "a whole result has no trailing .0");
    assert_eq!(format_number(-0.5), "-0.5");
}

#[test]
fn a_huge_number_formats_as_itself() {
    assert_ne!(
        format_number(1e300),
        "inf",
        "scaling to hide noise must not overflow"
    );
    assert_eq!(format_number(1e300).parse::<f64>().ok(), Some(1e300));
}

#[test]
fn digit_grouping_and_exponents_parse() {
    assert_eq!(eval("1_000_000 / 4").as_deref(), Some("250000"));
    assert_eq!(eval("2e3 + 1").as_deref(), Some("2001"));
    assert_eq!(eval("1.5e-2").as_deref(), Some("0.015"));
    assert_eq!(eval(".5 * 4").as_deref(), Some("2"));
}

#[test]
fn an_app_name_is_not_a_calculation() {
    for query in [
        "firefox",
        "code",
        "2 + 3 firefox",
        "sqrt",
        "((1+2)",
        "1 +",
        "+",
        "",
        "   ",
        "log(",
    ] {
        assert!(
            calculate(query).is_err(),
            "'{query}' must not evaluate to a number"
        );
    }
}

#[test]
fn division_by_zero_is_not_an_answer() {
    assert!(calculate("1/0").is_err());
    assert!(calculate("5 / (3 - 3)").is_err());
}

#[test]
fn a_non_finite_result_is_not_an_answer() {
    for query in ["2^10000", "sqrt(-1)", "ln(0)", "1e308 * 10", "asin(2)"] {
        assert!(calculate(query).is_err(), "'{query}' has no finite answer");
    }
}

/// The conversion half of the launcher's calculator stays in the application: the evaluator only has to refuse a conversion, so the application can try it as one.
#[test]
fn a_conversion_is_not_arithmetic() {
    assert!(calculate("3 km in mi").is_err());
    assert!(calculate("100 c in f").is_err());
    assert!(calculate("photos in library").is_err());
}

/// The launcher's "a bare number is not worth echoing" rule is its own; the evaluator answers both, and what makes a query worth showing is the caller's to decide.
#[test]
fn a_bare_number_and_a_sum_both_evaluate() {
    assert_eq!(eval("2").as_deref(), Some("2"));
    assert_eq!(eval("42").as_deref(), Some("42"));
    assert_eq!(eval("2+2").as_deref(), Some("4"));
    assert!(calculate("sqrt(2)").is_ok());
    assert_eq!(eval(" 8 * 7 ").as_deref(), Some("56"));
    assert!(calculate("firefox").is_err());
}

#[test]
fn typographic_operators_are_read() {
    assert_eq!(eval("6 × 7").as_deref(), Some("42"));
    assert_eq!(eval("8 ÷ 2").as_deref(), Some("4"));
    assert_eq!(eval("5 − 3").as_deref(), Some("2"));
    assert_eq!(eval("2 · 3").as_deref(), Some("6"));
}

#[test]
fn an_expression_that_is_not_a_number_is_refused() {
    assert!(calculate("\"5\"").is_err());
    assert!(calculate("1 < 2").is_err());
    assert!(calculate("#fff").is_err());
}

#[test]
fn the_error_says_where() {
    let error = calculate("2 + 3 firefox").unwrap_err();
    assert_eq!(error.kind, ErrorKind::Syntax);
    assert_eq!(error.span, Span::new(6, 13));
}
