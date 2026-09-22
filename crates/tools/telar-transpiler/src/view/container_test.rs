fn transpiled(view: &str) -> String {
    let src = format!("[logic]\nlet open = signal(false);\n[view]\n{view}");
    crate::transpile_source(&src, "demo", None, None)
        .unwrap()
        .rust_code
}

#[test]
fn named_keys_become_the_constants_they_spell() {
    let code =
        transpiled("box focus_style(fill:#ffffff) consumes_keys:arrows,space\n    text \"x\"\n");
    assert!(
        code.contains(
            ".consumes_keys(|| ::telar::ConsumedKeys::ARROWS | ::telar::ConsumedKeys::SPACE)"
        ),
        "{code}"
    );
    assert!(!code.contains("compile_error!"), "{code}");
}

#[test]
fn a_parenthesised_list_may_be_spaced() {
    let code = transpiled("box on_focus:(|_f| ()) consumes_keys:(up down home)\n    text \"x\"\n");
    assert!(
        code.contains(
            ".consumes_keys(|| ::telar::ConsumedKeys::ARROW_UP | ::telar::ConsumedKeys::ARROW_DOWN | ::telar::ConsumedKeys::HOME)"
        ),
        "{code}"
    );
}

#[test]
fn none_keeps_nothing() {
    let code = transpiled("box on_focus:(|_f| ()) consumes_keys:none\n    text \"x\"\n");
    assert!(
        code.contains(".consumes_keys(|| ::telar::ConsumedKeys::EMPTY)"),
        "{code}"
    );
}

#[test]
fn a_misspelt_key_is_a_compile_error_not_a_key_handed_back() {
    let code = transpiled("box on_focus:(|_f| ()) consumes_keys:arows\n    text \"x\"\n");
    assert!(code.contains("compile_error!"), "{code}");
    assert!(code.contains("`arows`"), "{code}");
}

#[test]
fn a_signal_reading_expression_is_re_read_every_render() {
    let code = transpiled(
        "box on_focus:(|_f| ()) consumes_keys:(if $open { ConsumedKeys::ARROWS } else { ConsumedKeys::EMPTY })\n    text \"x\"\n",
    );
    assert!(code.contains(".consumes_keys("), "{code}");
    assert!(code.contains("move ||"), "{code}");
    assert!(code.contains("open.get()"), "{code}");
}

#[test]
fn declaring_keys_makes_a_plain_col_a_styled_container() {
    let code = transpiled("col consumes_keys:arrows\n    text \"x\"\n");
    assert!(code.contains("StyledContainer::new("), "{code}");
}
