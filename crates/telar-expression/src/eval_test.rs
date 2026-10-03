use geometry_core::Color;

use crate::{
    Closed, ErrorCode, ErrorKind, HostError, Pattern, Reference, Registry, Signature, Span, Type,
    Value, compile,
};

fn run(source: &str) -> Result<Value, crate::Error> {
    compile(source, &Closed, &Registry::standard())
        .unwrap_or_else(|errors| panic!("{source:?}: {errors}"))
        .evaluate(&Closed)
}

fn value(source: &str) -> Value {
    run(source).unwrap_or_else(|error| panic!("{source:?}: {error}"))
}

fn failure(source: &str) -> (&str, ErrorCode) {
    let error = run(source).expect_err(source);
    assert_eq!(error.kind, ErrorKind::Evaluation, "{error:?}");
    (&source[error.span.range()], error.code)
}

#[test]
fn a_runtime_failure_is_located_at_its_cause() {
    assert_eq!(failure("5 / (3 - 3)").0, "(3 - 3)");
    assert_eq!(failure("1 + 10^400").0, "10^400");
    assert_eq!(failure("1 + sqrt(-1)").0, "sqrt(-1)");
    assert_eq!(failure("at({1, 2}, 5)").0, "5");
    assert_eq!(failure("at({1, 2}, 0.5)").0, "0.5");
    assert_eq!(failure("fmt(\"{} {}\", 1)").0, "\"{} {}\"");
    assert_eq!(failure("round(1, 1.5)").0, "1.5");
}

#[test]
fn a_runtime_failure_is_a_code_with_its_arguments() {
    assert_eq!(failure("1 / 0").1, ErrorCode::DivisionByZero);
    assert_eq!(
        failure("sqrt(-1)").1,
        ErrorCode::NoFiniteResult {
            name: "sqrt".into()
        }
    );
    assert_eq!(
        failure("at({1, 2}, 5)").1,
        ErrorCode::PositionOutside {
            position: 5,
            len: 2
        }
    );
    assert_eq!(
        failure("fmt(\"{} {}\", 1)").1,
        ErrorCode::FormatMissingValue { index: 1, given: 1 }
    );
    assert_eq!(failure("round(1, 1.5)").1, ErrorCode::DecimalsNotWhole);
    assert_eq!(failure("1 / 0").1.to_string(), "division by zero");
}

#[test]
fn the_untaken_side_is_never_evaluated() {
    assert_eq!(value("false && 1/0 == 1"), Value::Bool(false));
    assert_eq!(value("true || 1/0 == 1"), Value::Bool(true));
    assert_eq!(value("if(true, 1, 1/0)"), Value::Number(1.0));
    assert_eq!(value("if(false, 1/0, 2)"), Value::Number(2.0));
}

#[test]
fn comparison_and_equality_cover_every_type() {
    assert_eq!(value("{1, 2} == {1, 2}"), Value::Bool(true));
    assert_eq!(value("#f00 == #ff0000"), Value::Bool(true));
    assert_eq!(value("\"a\" != \"b\""), Value::Bool(true));
    assert_eq!(value("\"apple\" < \"banana\""), Value::Bool(true));
    assert_eq!(value("2 >= 2 && 1 <= 2 && 3 > 2"), Value::Bool(true));
    assert_eq!(value("{} == {}"), Value::Bool(true));
}

#[test]
fn texts_join_with_plus() {
    assert_eq!(value("\"a\" + 'b' + \"\\n\""), Value::text("ab\n"));
}

#[test]
fn text_functions() {
    assert_eq!(value("upper(\"héllo\")"), Value::text("HÉLLO"));
    assert_eq!(value("lower(\"ABC\")"), Value::text("abc"));
    assert_eq!(value("trim(\"  x \")"), Value::text("x"));
    assert_eq!(
        value("len(\"héllo\")"),
        Value::Number(5.0),
        "characters, not bytes"
    );
    assert_eq!(value("substring(\"héllo\", 1, 3)"), Value::text("él"));
    assert_eq!(value("substring(\"héllo\", 3)"), Value::text("lo"));
    assert_eq!(
        value("substring(\"abc\", -5, 99)"),
        Value::text("abc"),
        "positions are clamped to the text"
    );
    assert_eq!(value("substring(\"abc\", 2, 1)"), Value::text(""));
    assert_eq!(
        value("replace(\"a-b-c\", \"-\", \"+\")"),
        Value::text("a+b+c")
    );
    assert_eq!(value("contains(\"hello\", \"ell\")"), Value::Bool(true));
}

#[test]
fn fmt_fills_placeholders_in_order_or_by_position() {
    assert_eq!(value("fmt(\"{} of {}\", 1, 2)"), Value::text("1 of 2"));
    assert_eq!(value("fmt(\"{1}{0}\", \"a\", \"b\")"), Value::text("ba"));
    assert_eq!(value("fmt(\"{{}}\")"), Value::text("{}"));
    assert_eq!(
        value("fmt(\"{}% {} {} {}\", 0.1 + 0.2, true, #ff000080, {1, 2})"),
        Value::text("0.3% true #ff000080 1, 2")
    );
    assert!(run("fmt(\"{\")").is_err());
    assert!(run("fmt(\"}\")").is_err());
    assert!(run("fmt(\"{x}\", 1)").is_err());
    assert!(run("fmt(\"{99999999999999999999999}\", 1)").is_err());
}

#[test]
fn a_text_cannot_grow_without_bound() {
    let mut source = "\"aaaaaaaa\"".to_string();
    for _ in 0..12 {
        source = format!("replace({source}, \"\", \"aaaaaaaa\")");
    }
    assert_eq!(
        failure(&source).1,
        ErrorCode::TextTooLong {
            limit: crate::MAX_TEXT_BYTES
        }
    );
    let mut source = "\"x\"".to_string();
    for _ in 0..25 {
        source = format!("fmt(\"{{}}{{}}{{}}{{}}\", {source}, {source}, {source}, {source})");
        if source.len() > 4096 {
            break;
        }
    }
    let _ = run(&source);
}

#[test]
fn math_functions() {
    assert_eq!(value("round(1.23456, 2)"), Value::Number(1.23));
    assert_eq!(value("round(1234, -2)"), Value::Number(1200.0));
    assert_eq!(value("min(3, 1, 2)"), Value::Number(1.0));
    assert_eq!(value("max(3, 1, 2)"), Value::Number(3.0));
    assert_eq!(value("max(7)"), Value::Number(7.0));
    assert_eq!(value("pow(2, 8)"), Value::Number(256.0));
    assert_eq!(
        value("abs(-2) + floor(1.7) + ceil(1.2)"),
        Value::Number(5.0)
    );
    assert!(
        run("round(1, 400)").is_err(),
        "a scale that overflows is an error, not NaN"
    );
}

#[test]
fn list_functions() {
    assert_eq!(value("len({1, 2, 3})"), Value::Number(3.0));
    assert_eq!(value("at({1, 2, 3}, -1)"), Value::Number(3.0));
    assert_eq!(value("at({\"a\", \"b\"}, 0)"), Value::text("a"));
    assert_eq!(value("join({1, 2.5, 3}, \", \")"), Value::text("1, 2.5, 3"));
    assert_eq!(value("join({}, \"-\")"), Value::text(""));
    assert!(run("at({}, 0)").is_err());
}

#[test]
fn colour_functions() {
    let contrast = value("contrast(#000, #fff)").as_number().unwrap();
    assert!((contrast - 21.0).abs() < 1e-4, "{contrast}");
    assert_eq!(value("mix(#f00, #00f, 0)"), Value::Color(Color::RED));
    assert_eq!(value("mix(#f00, #00f, 150%)"), Value::Color(Color::BLUE));
    let Value::Color(half) = value("alpha(#f00, 50%)") else {
        panic!("alpha gives a colour");
    };
    assert_eq!(half.a, 0.5);
    assert_eq!(value("alpha(#f00, 50%)").to_string(), "#ff000080");
    assert_eq!(value("#00ff00").to_string(), "#00ff00");
}

#[test]
fn a_reference_is_read_through_the_resolver_and_held_to_its_type() {
    let environment = |_: &Reference| Ok(Type::Number);
    let registry = Registry::standard();
    let compiled = compile("$a + 1", &environment, &registry).unwrap();
    let span_of = |result: Result<Value, crate::Error>| result.unwrap_err().span;
    assert_eq!(
        compiled.evaluate(&|_: &Reference| Ok(Value::Number(2.0))),
        Ok(Value::Number(3.0))
    );
    assert_eq!(
        span_of(compiled.evaluate(&|_: &Reference| Ok(Value::text("2")))),
        Span::new(0, 2),
        "a value of the wrong type from the host is an error at the reference"
    );
    assert_eq!(
        span_of(compiled.evaluate(&|_: &Reference| Ok(Value::Number(f64::NAN)))),
        Span::new(0, 2)
    );
    let error = compiled
        .evaluate(&|_: &Reference| Err(HostError::new("pending", "no reading yet")))
        .unwrap_err();
    assert_eq!(
        (error.span, error.message().as_str()),
        (Span::new(0, 2), "no reading yet")
    );
}

#[test]
fn a_host_function_that_breaks_its_signature_is_an_error() {
    let registry = Registry::standard().with(|registry| {
        registry
            .function(
                "liar",
                Signature::new([], Type::Number),
                "Claims a number, gives text",
                |_| Ok(Value::text("x")),
            )
            .function("nan", Signature::new([], Type::Number), "Gives NaN", |_| {
                Ok(Value::Number(f64::NAN))
            })
            .function(
                "wrong_list",
                Signature::new([], Pattern::list(Pattern::Exact(Type::Number))),
                "Gives a list of texts",
                |_| Ok(Value::list([Value::text("x")])),
            );
    });
    for source in ["liar() + 1", "nan()", "len(wrong_list())"] {
        let compiled = compile(source, &Closed, &registry).unwrap();
        let error = compiled.evaluate(&Closed).expect_err(source);
        assert_eq!(error.kind, ErrorKind::Evaluation);
    }
}

#[test]
fn every_type_has_an_empty_value_that_fits_it() {
    for ty in [
        Type::Number,
        Type::Text,
        Type::Bool,
        Type::Color,
        Type::list(Type::Text),
    ] {
        let empty = Value::empty(&ty).unwrap();
        assert!(empty.conforms_to(&ty), "{empty:?} is not a {ty}");
    }
    assert_eq!(Value::empty(&Type::Never), None);
}
