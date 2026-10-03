use crate::{ErrorCode, HostError, Reference, Registry, Type, Value, compile, render};

use super::*;

/// xorshift64*: deterministic, so a failing case reproduces from its index alone.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

const VOCABULARY: &[&str] = &[
    "0",
    "1",
    "2.5",
    "1e308",
    "1_000",
    ".5",
    "1.2.3",
    "10%",
    "%",
    "+",
    "-",
    "*",
    "/",
    "^",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    ",",
    "==",
    "!=",
    "<",
    "<=",
    ">",
    ">=",
    "&&",
    "||",
    "!",
    "\"a\"",
    "'b'",
    "\"",
    "'",
    "\"{}{0}{1}{\"",
    "\"\\q\"",
    "#fff",
    "#12345678",
    "#zz",
    "#",
    "$n",
    "$t",
    "$l",
    "$c",
    "$b",
    "$x.y",
    "$n.",
    "$",
    "true",
    "FALSE",
    "if",
    "IF",
    "pi",
    "π",
    "e",
    "sqrt",
    "fmt",
    "len",
    "at",
    "join",
    "mix",
    "alpha",
    "contrast",
    "round",
    "min",
    "max",
    "pow",
    "replace",
    "substring",
    "upper",
    "contains",
    "unknown",
    " ",
    "−",
    "×",
    "÷",
    "é",
    "\\",
    "=",
    "&",
    "|",
    ".",
    "\n",
    "\t",
    "_",
    "2e",
    "1e-400",
];

fn environment(reference: &Reference) -> Result<Type, HostError> {
    match reference.dotted().as_str() {
        "n" => Ok(Type::Number),
        "t" => Ok(Type::Text),
        "l" => Ok(Type::list(Type::Number)),
        "c" => Ok(Type::Color),
        "b" => Ok(Type::Bool),
        _ => Err(HostError::new(
            "unknown",
            format!("no reading called {reference}"),
        )),
    }
}

fn resolver(reference: &Reference) -> Result<Value, HostError> {
    match reference.dotted().as_str() {
        "n" => Ok(Value::Number(f64::MAX)),
        "t" => Ok(Value::text("ünïcode")),
        "l" => Ok(Value::list([Value::Number(1.0), Value::Number(-0.0)])),
        "c" => Ok(Value::Color(geometry_core::Color::TRANSPARENT)),
        "b" => Err(HostError::new("pending", "not yet")),
        _ => Err(HostError::new("unreachable", "unreachable")),
    }
}

/// Runs `source` through every stage, checking that whatever comes back is located inside the source.
fn exercise(registry: &Registry, source: &str) {
    let check = |error: &crate::Error| {
        assert!(
            error.span.start <= error.span.end
                && error.span.end <= source.len()
                && source.is_char_boundary(error.span.start)
                && source.is_char_boundary(error.span.end),
            "{source:?}: span {:?} is not inside the source",
            error.span
        );
        render(source, error);
    };
    match compile(source, &environment, registry) {
        Ok(compiled) => {
            if let Err(error) = compiled.evaluate(&resolver) {
                check(&error);
            }
        }
        Err(errors) => errors.iter().for_each(check),
    }
}

#[test]
fn token_soup_never_panics() {
    let registry = Registry::standard();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..20_000 {
        let length = 1 + rng.below(40);
        let mut source = String::new();
        for _ in 0..length {
            source.push_str(VOCABULARY[rng.below(VOCABULARY.len())]);
            if rng.below(3) == 0 {
                source.push(' ');
            }
        }
        exercise(&registry, &source);
    }
}

#[test]
fn random_bytes_never_panic() {
    let registry = Registry::standard();
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    for _ in 0..20_000 {
        let length = rng.below(64);
        let bytes: Vec<u8> = (0..length).map(|_| rng.next() as u8).collect();
        exercise(&registry, &String::from_utf8_lossy(&bytes));
    }
}

#[test]
fn hostile_nesting_is_an_error_not_a_stack_overflow() {
    let registry = Registry::standard();
    let deep = 100_000;
    for source in [
        format!("{}1{}", "(".repeat(deep), ")".repeat(deep)),
        format!("{}1", "-".repeat(deep)),
        format!("{}true", "!".repeat(deep)),
        format!("{}1", "1+".repeat(deep)),
        format!("{}1", "2^".repeat(deep)),
        format!("{}1{}", "{".repeat(deep), "}".repeat(deep)),
        format!("{}1{}", "sqrt(".repeat(deep), ")".repeat(deep)),
        format!("{}1{}", "[".repeat(deep), "]".repeat(deep)),
        "(".repeat(deep),
        format!("{}%", "1".repeat(deep)),
    ] {
        let errors = compile(&source, &environment, &registry)
            .expect_err("nesting this deep must be refused");
        assert!(
            errors.first().code == crate::ErrorCode::TooDeep
                || errors.first().kind == crate::ErrorKind::Syntax,
            "{}",
            errors.first()
        );
    }
}

#[test]
fn nesting_up_to_the_limit_still_parses() {
    let depth = MAX_DEPTH as usize / 2;
    let source = format!("{}1{}", "(".repeat(depth), ")".repeat(depth));
    assert!(parse(&source).is_ok());
    let chain = format!("{}1", "1+".repeat(depth));
    assert!(parse(&chain).is_ok());
}

#[test]
fn every_node_spans_what_it_was_written_as() {
    let source = "max(1, $a.b) * (2 + 3)";
    let expr = parse(source).unwrap();
    assert_eq!(expr.span, Span::new(0, source.len()));
    let ExprKind::Binary {
        left,
        right,
        op_span,
        ..
    } = &expr.kind
    else {
        panic!("expected a product, got {expr:?}");
    };
    assert_eq!(&source[left.span.range()], "max(1, $a.b)");
    assert_eq!(&source[op_span.range()], "*");
    assert_eq!(
        &source[right.span.range()],
        "(2 + 3)",
        "a bracketed group spans its brackets"
    );
}

#[test]
fn a_reference_reads_its_whole_path() {
    let expr = parse("$media.art.url").unwrap();
    assert_eq!(
        expr.kind,
        ExprKind::Reference(Reference::new("media", ["art", "url"]))
    );
}

#[test]
fn syntax_errors_point_at_the_problem() {
    let at = |source: &str| {
        let error = parse(source).unwrap_err();
        (error.span, error.code)
    };
    assert_eq!(
        at("1 +").0,
        Span::point(3),
        "an operand was expected at the end"
    );
    assert_eq!(at("((1+2)").0, Span::point(6));
    assert_eq!(at("2 + 3 firefox").0, Span::new(6, 13));
    assert_eq!(at("\"abc").0, Span::new(0, 4));
    assert_eq!(at("#ggg").0, Span::new(0, 4));
    assert_eq!(at("1 = 2").0, Span::new(2, 3));
    assert_eq!(
        at("1 = 2").1,
        ErrorCode::LoneOperator {
            found: '=',
            suggestion: "=="
        }
    );
    assert_eq!(at("if").0, Span::new(0, 2));
    assert_eq!(at("if(true, 1)").0, Span::new(0, 11));
    assert_eq!(at("1 ? 2").0, Span::new(2, 3));
    assert_eq!(at("1.2.3").0, Span::new(0, 5));
    assert_eq!(at("$ x").0, Span::new(0, 1));
    assert_eq!(at("\"a\\qb\"").0, Span::new(2, 4));
}

#[test]
fn a_list_may_end_with_a_comma_and_may_be_empty() {
    assert!(matches!(parse("{1, 2,}").unwrap().kind, ExprKind::List(items) if items.len() == 2));
    assert!(matches!(parse("{}").unwrap().kind, ExprKind::List(items) if items.is_empty()));
    assert!(parse("{,}").is_err());
}
