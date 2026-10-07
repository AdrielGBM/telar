use crate::{CallError, Closed, ErrorCode, ErrorKind, HostError, Pattern, Signature};

use super::*;

fn environment(reference: &Reference) -> Result<Type, HostError> {
    match reference.dotted().as_str() {
        "level" => Ok(Type::Number),
        "title" => Ok(Type::Text),
        "accent" => Ok(Type::Color),
        "playing" => Ok(Type::Bool),
        "tags" => Ok(Type::list(Type::Text)),
        other => Err(
            HostError::new("undeclared", format!("nothing called ${other} is declared")).arg(other),
        ),
    }
}

fn errors(source: &str) -> Vec<(Span, ErrorCode)> {
    match compile(source, &environment, &Registry::standard()) {
        Ok(compiled) => panic!("{source:?} compiled to {compiled:?}"),
        Err(errors) => errors
            .into_iter()
            .inspect(|error| assert_eq!(error.kind, ErrorKind::Type, "{error:?}"))
            .map(|error| (error.span, error.code))
            .collect(),
    }
}

fn spans(source: &str) -> Vec<&str> {
    errors(source)
        .into_iter()
        .map(|(span, _)| &source[span.range()])
        .collect()
}

fn ty(source: &str) -> Type {
    compile(source, &environment, &Registry::standard())
        .unwrap_or_else(|errors| panic!("{source:?}: {errors}"))
        .ty()
        .clone()
}

#[test]
fn a_mismatched_operand_is_blamed_where_it_is_written() {
    assert_eq!(spans("1 + \"a\""), ["\"a\""]);
    assert_eq!(spans("$level * $title"), ["$title"]);
    assert_eq!(spans("-$title"), ["$title"]);
    assert_eq!(spans("!$level"), ["$level"]);
    assert_eq!(spans("$playing && 1"), ["1"]);
    assert_eq!(spans("\"a\"%"), ["\"a\""]);
    assert_eq!(spans("\"a\" + 1"), ["1"]);
}

#[test]
fn each_mistake_is_reported_once_and_in_order() {
    assert_eq!(spans("\"a\" * 2 + sqrt"), ["\"a\"", "sqrt"]);
    assert_eq!(
        spans("unknown(1) + 1"),
        ["unknown"],
        "a failed call is not blamed again by the sum around it"
    );
}

#[test]
fn an_undeclared_reference_carries_the_hosts_message() {
    let found = errors("$volume + 1");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, Span::new(0, 7));
    assert_eq!(
        found[0].1,
        ErrorCode::Host(
            HostError::new("undeclared", "nothing called $volume is declared").arg("volume")
        )
    );
    assert_eq!(found[0].1.to_string(), "nothing called $volume is declared");
}

#[test]
fn names_are_explained_by_what_they_are() {
    assert_eq!(errors("sqrt")[0].1.key(), "name_is_function");
    assert_eq!(errors("pi(2)")[0].1.key(), "name_is_constant");
    assert_eq!(
        errors("firefox")[0].1,
        ErrorCode::UnknownName {
            name: "firefox".to_string()
        }
    );
    assert_eq!(spans("frobnicate(1, 2)"), ["frobnicate"]);
}

#[test]
fn an_argument_of_the_wrong_type_is_blamed_alone() {
    assert_eq!(spans("mix(#fff, 1, 0.5)"), ["1"]);
    assert_eq!(spans("round($title)"), ["$title"]);
    assert_eq!(spans("max(1, 2, \"3\")"), ["\"3\""]);
}

#[test]
fn a_wrong_count_or_an_unmatched_overload_blames_the_call() {
    let found = errors("sqrt(1, 2)");
    assert_eq!(found[0].0, Span::new(0, 10));
    assert_eq!(
        found[0].1,
        ErrorCode::ArgumentCount {
            name: "sqrt".to_string(),
            takes: 1,
            at_least: false,
            given: 2
        }
    );
    assert_eq!(
        found[0].1.to_string(),
        "`sqrt` takes 1 argument, but 2 were given"
    );
    let found = errors("len(1)");
    assert_eq!(found[0].0, Span::new(0, 6));
    assert_eq!(found[0].1.key(), "no_matching_form");
    assert!(
        found[0]
            .1
            .to_string()
            .starts_with("no form of `len` takes (number); it takes len("),
        "{}",
        found[0].1
    );
}

#[test]
fn branches_and_list_items_must_agree() {
    assert_eq!(spans("if($playing, 1, \"a\")"), ["\"a\""]);
    assert_eq!(spans("if(1, 2, 3)"), ["1"]);
    assert_eq!(spans("{1, 2, \"a\"}"), ["\"a\""]);
    assert_eq!(spans("$level == $title"), ["$title"]);
    assert_eq!(spans("#fff < #000"), ["<"]);
}

#[test]
fn types_are_inferred_through_every_construct() {
    assert_eq!(ty("{}"), Type::list(Type::Never));
    assert_eq!(ty("{{}, {1}}"), Type::list(Type::list(Type::Number)));
    assert_eq!(ty("if($playing, {}, {1})"), Type::list(Type::Number));
    assert_eq!(ty("at($tags, 0)"), Type::Text);
    assert_eq!(ty("at({}, 0)"), Type::Never);
    assert_eq!(ty("mix($accent, #000, 50%)"), Type::Color);
    assert_eq!(ty("$title + \"!\""), Type::Text);
    assert_eq!(ty("len($tags) > 2 || $playing"), Type::Bool);
    assert_eq!(ty("IF(TRUE, Sqrt(4), 0)"), Type::Number);
}

#[test]
fn every_reference_is_listed_once_in_order() {
    let compiled = compile(
        "if($playing, $level, $level + len($title))",
        &environment,
        &Registry::standard(),
    )
    .unwrap();
    let names: Vec<String> = compiled
        .references()
        .iter()
        .map(Reference::dotted)
        .collect();
    assert_eq!(names, ["playing", "level", "title"]);
}

#[test]
fn require_checks_the_result_against_what_the_property_takes() {
    let registry = Registry::standard();
    let number = compile("1 + 2", &Closed, &registry).unwrap();
    assert!(number.clone().require(&Type::Number).is_ok());
    let error = number.require(&Type::Color).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Type);
    assert_eq!(error.span, Span::new(0, 5));
    assert!(
        compile("{}", &Closed, &registry)
            .unwrap()
            .require(&Type::list(Type::Text))
            .is_ok(),
        "an empty list is a list of anything"
    );
}

#[test]
fn a_host_family_adds_functions_and_can_replace_a_form() {
    let registry = Registry::standard().with(|registry| {
        registry
            .function(
                "double",
                Signature::new([Pattern::Exact(Type::Number)], Type::Number),
                "Twice the number",
                |a| Ok(Value::Number(a.number(0)? * 2.0)),
            )
            .function(
                "upper",
                Signature::new([Pattern::Exact(Type::Text)], Type::Text),
                "Shouts",
                |a| Ok(Value::text(format!("{}!", a.text(0)?.to_uppercase()))),
            )
            .function(
                "refuse",
                Signature::new([Pattern::Exact(Type::Number)], Type::Number),
                "Always fails on its argument",
                |_| {
                    Err(CallError::argument(
                        0,
                        ErrorCode::Host(HostError::new("refused", "no")),
                    ))
                },
            )
            .constant("answer", Value::Number(42.0), "The answer");
    });
    let evaluate = |source: &str| {
        compile(source, &Closed, &registry)
            .unwrap()
            .evaluate(&Closed)
    };
    assert_eq!(evaluate("double(answer)"), Ok(Value::Number(84.0)));
    assert_eq!(evaluate("upper(\"hi\")"), Ok(Value::text("HI!")));
    let error = evaluate("refuse(1 + 1)").unwrap_err();
    assert_eq!(
        error.span,
        Span::new(7, 12),
        "the argument at fault, not the call"
    );
    assert!(
        registry
            .functions()
            .any(|function| function.name() == "double")
    );
}

#[test]
#[should_panic(expected = "cannot be named")]
fn a_reserved_word_cannot_be_registered() {
    Registry::new().function("if", Signature::new([], Type::Number), "", |_| {
        Ok(Value::Number(0.0))
    });
}

#[test]
fn a_signature_reads_as_its_shape() {
    let at = Signature::new(
        [Pattern::list(Pattern::Var(0)), Pattern::Exact(Type::Number)],
        Pattern::Var(0),
    );
    assert_eq!(at.to_string(), "(list of T, number) -> T");
    let max = Signature::new([Pattern::Exact(Type::Number)], Type::Number)
        .rest(Pattern::Exact(Type::Number));
    assert_eq!(max.to_string(), "(number, …number) -> number");
}
