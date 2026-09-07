use super::*;

fn idents(expr: &str) -> Vec<String> {
    free_idents(expr).unwrap_or_else(|| panic!("`{expr}` did not parse"))
}

#[test]
fn a_field_a_method_and_a_path_segment_are_not_bindings() {
    assert_eq!(idents("seat(&desk, id).x"), ["seat", "desk", "id"]);
    assert_eq!(idents("crate::scale::md()"), Vec::<String>::new());
    assert_eq!(idents("value.clamp(lo, hi)"), ["value", "lo", "hi"]);
    assert_eq!(idents("Style { pad, ..base }"), ["pad", "base"]);
}

#[test]
fn a_closure_parameter_shadows_the_scope_it_sits_in() {
    assert_eq!(
        idents("items.iter().map(|x| x + offset)"),
        ["items", "offset"]
    );
    assert_eq!(idents("|n| { let m = n * 2; m + k }"), ["k"]);
}

#[test]
fn a_pattern_binds_every_name_it_introduces() {
    assert_eq!(
        idents("match slot { Some((a, b)) => a + b, None => fallback }"),
        ["slot", "fallback"]
    );
}

/// `$` is the markup's sugar and not Rust, so a value still carrying one has to be substituted before it can be read — saying so is what keeps the caller's fallback scan reachable.
#[test]
fn text_that_is_not_an_expression_says_so_instead_of_guessing() {
    assert_eq!(free_idents("$count.get()"), None);
    assert_eq!(free_idents("col gap:8"), None);
    assert_eq!(idents("12px"), Vec::<String>::new());
}
