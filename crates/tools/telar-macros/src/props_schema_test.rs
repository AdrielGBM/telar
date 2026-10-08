use syn::DeriveInput;

fn expand_str(source: &str) -> syn::Result<String> {
    Ok(crate::props::expand(syn::parse_str::<DeriveInput>(source)?)?.to_string())
}

fn schema_of(source: &str) -> String {
    let out = expand_str(source).expect("the struct derives");
    let start = out
        .find(":: telar :: __previews !")
        .unwrap_or_else(|| panic!("no gated schema in: {out}"));
    out[start..].to_owned()
}

fn control_error(control: &str) -> String {
    expand_str(&format!(
        "struct P {{ #[props(control = {control})] value: f32 }}"
    ))
    .expect_err("the control is refused")
    .to_string()
}

#[test]
fn the_schema_impl_is_gated_behind_the_previews_feature() {
    let schema = schema_of("struct LabelProps { text: String }");
    assert!(
        schema.contains(":: telar :: preview :: HasPropsSchema for LabelProps"),
        "{schema}"
    );
}

#[test]
fn struct_and_field_docs_are_unindented_and_joined() {
    let schema = schema_of(
        "/// A label.\n///\n/// Shown once.\nstruct LabelProps {\n    /// The text.\n    ///   Indented.\n    text: String,\n}",
    );
    assert!(
        schema.contains(r#"PropsSchema :: new ("LabelProps" , "A label.\n\nShown once.")"#),
        "{schema}"
    );
    assert!(
        schema.contains(r#". doc ("The text.\n  Indented.")"#),
        "{schema}"
    );
}

#[test]
fn a_props_doc_attribute_wins_over_the_doc_comment() {
    let schema = schema_of(
        "struct LabelProps { /// For rustdoc.\n #[props(doc = \"For the explorer.\")] text: String }",
    );
    assert!(
        schema.contains(r#". doc ("For the explorer.")"#),
        "{schema}"
    );
    assert!(!schema.contains("For rustdoc."), "{schema}");
}

#[test]
fn a_computed_doc_is_spliced_in_by_concat() {
    let schema = schema_of(
        "/// Intro.\n#[doc = include_str!(\"x.md\")]\nstruct LabelProps { text: String }",
    );
    assert!(
        schema.contains(r#":: core :: concat ! ("Intro." , include_str ! ("x.md"))"#),
        "{schema}"
    );
}

#[test]
fn types_and_defaults_are_written_as_source() {
    let schema = schema_of(
        "struct ButtonProps {\n label: Option<Reactive<String>>,\n #[props(into, default = Reactive::of(|| Color::TRANSPARENT))] fill: Reactive<Color>,\n #[props(default, some)] hint: Option<&'static str>,\n}",
    );
    assert!(
        schema.contains(r#"PropField :: new ("label" , "Option<Reactive<String>>""#),
        "{schema}"
    );
    assert!(
        schema.contains(r#"PropDefault :: Expr ("Reactive::of(|| Color::TRANSPARENT)")"#),
        "{schema}"
    );
    assert!(schema.contains(r#""Option<&'static str>""#), "{schema}");
    assert!(schema.contains("PropDefault :: TypeDefault"), "{schema}");
    assert!(schema.contains(". takes_into ()"), "{schema}");
    assert!(schema.contains(". takes_some ()"), "{schema}");
}

#[test]
fn a_required_prop_names_no_default() {
    let schema = schema_of("struct LabelProps { text: String }");
    assert!(!schema.contains("PropDefault"), "{schema}");
}

#[test]
fn a_field_without_an_override_is_probed() {
    let schema = schema_of("struct LabelProps { text: String }");
    assert!(
        schema.contains(":: telar :: __preview_control ! (String)"),
        "{schema}"
    );
    assert!(!schema.contains("refine"), "{schema}");
}

#[test]
fn a_control_override_refines_the_probed_control() {
    let schema = schema_of(
        "struct P { #[props(control = range(0, 100).step(0.5))] value: f32, #[props(control = multiline)] body: String }",
    );
    assert!(
        schema.contains("refine (:: telar :: __preview_control ! (f32)"),
        "{schema}"
    );
    assert!(
        schema.contains("control . range (0f64 , 100f64) . step (0.5f64)"),
        "{schema}"
    );
    assert!(schema.contains("control . multiline ()"), "{schema}");
}

#[test]
fn negative_bounds_are_read() {
    let schema = schema_of("struct P { #[props(control = range(-1, 1.5))] value: f32 }");
    assert!(schema.contains(". range (- 1f64 , 1.5f64)"), "{schema}");
}

#[test]
fn read_only_replaces_the_control() {
    let schema = schema_of("struct P { #[props(control = read_only)] value: f32 }");
    assert!(
        schema.contains(":: telar :: preview :: ControlKind :: ReadOnly"),
        "{schema}"
    );
    assert!(!schema.contains("__preview_control"), "{schema}");
}

#[test]
fn props_attributes_spread_over_several_attributes_all_count() {
    let schema = schema_of(
        "struct P { #[props(into)] #[props(default = 1.0)] #[props(control = range(0, 2))] value: f32 }",
    );
    assert!(schema.contains(". takes_into ()"), "{schema}");
    assert!(
        schema.contains(r#"PropDefault :: Expr ("1.0")"#),
        "{schema}"
    );
    assert!(schema.contains(". range (0f64 , 2f64)"), "{schema}");
}

#[test]
fn an_unknown_control_is_refused_with_the_known_ones() {
    let error = control_error("slider");
    assert!(error.contains("unknown control `slider`"), "{error}");
    assert!(error.contains("`range(min, max)`"), "{error}");
}

#[test]
fn a_malformed_control_is_refused() {
    assert!(control_error("range(1)").contains("`range` takes 2 numbers"));
    assert!(control_error("range(5, 1)").contains("min is above its max"));
    assert!(control_error("step(0)").contains("above zero"));
    assert!(control_error("range(0, MAX)").contains("number literal"));
    assert!(control_error("multiline(3)").contains("takes nothing"));
    assert!(control_error("range(0, 1).read_only()").contains("stands alone"));
    assert!(control_error("\"slider\"").contains("the controls are"));
}

#[test]
fn an_unknown_prop_attribute_lists_the_known_ones() {
    let error = expand_str("struct P { #[props(hidden)] value: f32 }")
        .expect_err("not a prop attribute")
        .to_string();
    assert!(error.contains("`doc` and `control`"), "{error}");
}

#[test]
fn a_generic_props_struct_is_refused() {
    for source in [
        "struct P<T> { value: T }",
        "struct P<'a> { value: &'a str }",
    ] {
        let error = expand_str(source).expect_err("generic props").to_string();
        assert!(error.contains("no generic parameters"), "{source}: {error}");
    }
}
