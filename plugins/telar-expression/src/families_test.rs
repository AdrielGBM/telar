use super::*;

#[test]
fn the_standard_registry_carries_every_family_and_nothing_more() {
    let registry = Registry::standard();
    let mut names: Vec<&str> = registry
        .functions()
        .map(|function| function.name())
        .collect();
    names.dedup();
    assert_eq!(
        names,
        [
            "abs",
            "acos",
            "alpha",
            "asin",
            "at",
            "atan",
            "cbrt",
            "ceil",
            "contains",
            "contrast",
            "cos",
            "exp",
            "floor",
            "fmt",
            "join",
            "len",
            "ln",
            "log",
            "log10",
            "log2",
            "lower",
            "max",
            "min",
            "mix",
            "pow",
            "replace",
            "round",
            "sin",
            "sqrt",
            "substring",
            "tan",
            "trim",
            "upper",
        ]
    );
    let constants: Vec<&str> = registry
        .constants()
        .map(|constant| constant.name.as_str())
        .collect();
    assert_eq!(constants, ["e", "pi", "tau", "π"]);
}

#[test]
fn every_function_says_what_it_does() {
    for function in Registry::standard().functions() {
        assert!(
            !function.summary().is_empty(),
            "{function:?} has no summary"
        );
    }
}
