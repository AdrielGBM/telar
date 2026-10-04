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

#[test]
fn a_destination_makes_a_row_a_link_box() {
    let code = transpiled("row to:Page::Project(\"telar\".into())\n    text \"Telar\"\n");
    assert!(code.contains("StyledContainer::new("), "{code}");
    assert!(
        code.contains(".to(move || Page::Project(\"telar\".into()))"),
        "{code}"
    );
    assert!(!code.contains("compile_error!"), "{code}");
}

#[test]
fn an_anchor_and_an_external_uri_are_written_as_they_read() {
    let code = transpiled("box to:anchor(\"contact\")\n    text \"x\"\n");
    assert!(code.contains(".to(move || anchor(\"contact\"))"), "{code}");
    let code = transpiled("box to:external(\"mailto:a@b.c\")\n    text \"x\"\n");
    assert!(
        code.contains(".to(move || external(\"mailto:a@b.c\"))"),
        "{code}"
    );
    assert!(!code.contains("compile_error!"), "{code}");
}

#[test]
fn a_destination_reading_state_follows_it() {
    let code = transpiled("box to:Page::Section($open)\n    text \"x\"\n");
    assert!(code.contains(".to("), "{code}");
    assert!(code.contains("open.get()"), "{code}");
}

#[test]
fn an_external_uri_without_a_scheme_is_a_build_error() {
    let code = transpiled("box to:external(\"example.com\")\n    text \"x\"\n");
    assert!(code.contains("compile_error!"), "{code}");
    assert!(code.contains("names no scheme"), "{code}");
}

#[test]
fn a_link_cannot_let_the_pointer_through() {
    let code = transpiled("box input_transparent to:anchor(\"a\")\n    text \"x\"\n");
    assert!(code.contains("compile_error!"), "{code}");
}

#[test]
fn a_role_a_person_operates_makes_the_box_a_control() {
    let code = transpiled("box role:button on_press:(|| ())\n    text \"x\"\n");
    assert!(code.contains(".control(::telar::Role::Button)"), "{code}");
    assert!(!code.contains(".role("), "{code}");
    assert!(!code.contains("compile_error!"), "{code}");
}

#[test]
fn a_region_is_described_and_not_made_a_tab_stop() {
    let code = transpiled("col role:navigation\n    text \"x\"\n");
    assert!(code.contains("Container::new("), "{code}");
    assert!(!code.contains("StyledContainer::new("), "{code}");
    assert!(code.contains(".role(::telar::Role::Navigation)"), "{code}");
    assert!(!code.contains(".control("), "{code}");
}

#[test]
fn a_control_role_makes_a_plain_row_a_styled_container() {
    let code = transpiled("row role:switch on_press:(|| ())\n    text \"x\"\n");
    assert!(code.contains("StyledContainer::new("), "{code}");
    assert!(code.contains(".control(::telar::Role::Switch)"), "{code}");
}

#[test]
fn an_alias_is_the_role_it_stands_for() {
    let code = transpiled("box role:toggle on_press:(|| ())\n    text \"x\"\n");
    assert!(code.contains(".control(::telar::Role::Switch)"), "{code}");
}

#[test]
fn a_state_is_read_after_the_box_became_the_control_that_carries_it() {
    let code = transpiled(
        "box role:switch toggled:$open label:\"Open\" on_press:(|| $open.set(!$open.get()))\n    text \"x\"\n",
    );
    let control = code
        .find(".control(::telar::Role::Switch)")
        .unwrap_or_else(|| panic!("a control: {code}"));
    let toggled = code
        .find(".toggled(")
        .unwrap_or_else(|| panic!("a state: {code}"));
    assert!(
        control < toggled,
        "`toggled` is only kept by a box already registered as a control: {code}"
    );
    assert!(code.contains("open.get()"), "{code}");
    assert!(code.contains(".a11y_label("), "{code}");
    assert!(!code.contains("compile_error!"), "{code}");
}

#[test]
fn a_toggle_button_is_a_button_with_a_state() {
    let code = transpiled("box role:button toggled:$open on_press:(|| ())\n    text \"B\"\n");
    assert!(code.contains(".control(::telar::Role::Button)"), "{code}");
    assert!(code.contains(".toggled("), "{code}");
    assert!(!code.contains("compile_error!"), "{code}");
}

#[test]
fn a_state_on_a_role_without_one_is_a_build_error() {
    for view in [
        "box toggled:$open on_press:(|| ())\n    text \"x\"\n",
        "box role:slider toggled:$open\n    text \"x\"\n",
        "col role:navigation toggled:$open\n    text \"x\"\n",
    ] {
        let code = transpiled(view);
        assert!(code.contains("compile_error!"), "{view}: {code}");
        assert!(code.contains("`toggled:` needs a role"), "{view}: {code}");
    }
}

#[test]
fn a_control_role_with_a_destination_is_a_build_error() {
    let code = transpiled("box role:button to:anchor(\"a\")\n    text \"x\"\n");
    assert!(code.contains("compile_error!"), "{code}");
    assert!(
        code.contains("a box with a destination is a link"),
        "{code}"
    );
    let code = transpiled("box role:link to:anchor(\"a\")\n    text \"x\"\n");
    assert!(!code.contains("compile_error!"), "{code}");
}
