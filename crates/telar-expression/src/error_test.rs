use crate::{Context, Found, HostError, Type};

use super::*;

fn unknown_name() -> ErrorCode {
    ErrorCode::UnknownName {
        name: "firefox".to_string(),
    }
}

#[test]
fn render_draws_a_caret_under_the_span() {
    let error = Error::mismatch(Span::new(6, 13), unknown_name());
    assert_eq!(
        render("2 + 3 firefox", &error),
        "type error: unknown name `firefox`\n  |\n1 | 2 + 3 firefox\n  |       ^^^^^^^"
    );
}

#[test]
fn render_finds_the_line_and_counts_columns_in_characters() {
    let source = "fmt(\"é\",\n  \"ü\" + $bad)";
    let error = Error::syntax(Span::new(19, 23), ErrorCode::UnknownEscape);
    assert_eq!(
        render(source, &error),
        "syntax error: unknown escape; write \\n, \\t, \\\\, \\\" or \\'\n  |\n2 |   \"ü\" + $bad)\n  |         ^^^^"
    );
}

#[test]
fn an_empty_span_at_the_end_still_gets_a_caret() {
    let error = Error::syntax(
        Span::point(3),
        ErrorCode::ExpectedValue { found: Found::End },
    );
    assert!(render("1 +", &error).ends_with("1 +\n  |    ^"));
}

#[test]
fn a_span_that_no_longer_fits_the_source_still_renders() {
    let error = Error::evaluation(Span::new(1, 400), ErrorCode::DivisionByZero);
    let rendered = render("é", &error);
    assert!(rendered.starts_with("evaluation error: division by zero"));
}

#[test]
fn errors_come_out_in_source_order() {
    let errors = Errors::from_vec(vec![
        Error::mismatch(
            Span::new(5, 6),
            ErrorCode::UnknownFunction { name: "b".into() },
        ),
        Error::mismatch(Span::new(0, 1), unknown_name()),
    ])
    .unwrap();
    assert_eq!(errors.first().code, unknown_name());
    assert_eq!(
        errors.to_string(),
        "unknown name `firefox`; unknown function `b`"
    );
    assert!(Errors::from_vec(Vec::new()).is_none());
}

#[test]
fn the_message_is_the_english_wording_of_the_code() {
    let error = Error::mismatch(Span::new(0, 1), unknown_name());
    assert_eq!(error.message(), "unknown name `firefox`");
    assert_eq!(error.to_string(), error.message());
    assert_eq!(error.code.key(), "unknown_name");
}

#[test]
fn a_code_carries_the_values_its_wording_is_built_from() {
    let code = ErrorCode::OperandType {
        context: Context::Operator("+"),
        expected: Type::Number,
        found: Type::Text,
    };
    assert_eq!(code.to_string(), "`+` needs number, but this is text");
    assert_eq!(code.key(), "operand_type");
    let ErrorCode::OperandType { found, .. } = code else {
        unreachable!();
    };
    assert_eq!(found, Type::Text);
}

#[test]
fn a_host_error_keeps_its_own_key_and_arguments() {
    let host = HostError::new("no_such_field", "$battery has no field `x`")
        .arg("battery")
        .arg("x");
    let code = ErrorCode::Host(host.clone());
    assert_eq!(code.key(), "host");
    assert_eq!(code.to_string(), "$battery has no field `x`");
    assert_eq!(host.args, ["battery", "x"]);
}
