use super::*;

fn expand_str(source: &str) -> syn::Result<String> {
    Ok(expand(syn::parse_str::<DeriveInput>(source)?)?.to_string())
}

fn squashed(text: &str) -> String {
    text.split_whitespace().collect()
}

#[test]
fn current_reads_the_theme_extension_of_the_struct() {
    let out = expand_str("struct TagTokens { radius: Option<f32> }").unwrap();
    assert!(
        out.contains(
            "pub fn current () -> Self { :: telar :: use_theme_extension :: < Self > () }"
        ),
        "{out}"
    );
}

#[test]
fn a_fallback_field_gets_a_getter_that_calls_it_when_unset() {
    let out =
        expand_str("struct T { #[token(or = shared::radius_item)] radius: Option<f32> }").unwrap();
    assert!(out.contains("pub fn radius (& self) -> f32"), "{out}");
    assert!(
        out.contains("self . radius . unwrap_or_else (shared :: radius_item)"),
        "{out}"
    );
}

#[test]
fn the_getter_returns_what_the_option_holds() {
    let out = expand_str("struct T { #[token(or = shared::surface)] background: Option<Color> }")
        .unwrap();
    assert!(out.contains("pub fn background (& self) -> Color"), "{out}");
}

#[test]
fn a_scaled_field_scales_a_set_value_and_hands_the_size_to_the_fallback() {
    let out = expand_str("struct T { #[token(or = shared::gap_md, scaled)] pad_x: Option<f32> }")
        .unwrap();
    let out = squashed(&out);
    assert!(
        out.contains(&squashed(
            "pub fn pad_x (& self , size : :: telar :: ControlSize) -> f32"
        )),
        "{out}"
    );
    assert!(
        out.contains(&squashed(
            "let fallback : fn (:: telar :: ControlSize) -> f32 = shared :: gap_md ;"
        )),
        "{out}"
    );
    assert!(
        out.contains(&squashed(
            "self . pad_x . map_or_else (|| fallback (size) , |set| set * size . scale ())"
        )),
        "{out}"
    );
}

#[test]
fn a_getter_carries_the_doc_of_its_field() {
    let out = expand_str(
        "struct T { /// The corner radius.\n #[token(or = shared::radius_item)] radius: Option<f32> }",
    )
    .unwrap();
    assert!(
        out.contains("# [doc = \" The corner radius.\"] pub fn radius"),
        "{out}"
    );
}

#[test]
fn a_field_without_an_attribute_gets_no_getter() {
    let out = expand_str("struct T { row_height: Option<f32> }").unwrap();
    assert!(!out.contains("fn row_height"), "{out}");
}

#[test]
fn a_fallback_on_a_field_that_is_not_an_option_is_an_error() {
    let err = expand_str("struct T { #[token(or = shared::radius_item)] radius: f32 }")
        .expect_err("nothing is unset");
    assert!(err.to_string().contains("`Option`"), "{err}");
}

#[test]
fn scaled_without_a_fallback_is_an_error() {
    let err = expand_str("struct T { #[token(scaled)] pad_x: Option<f32> }")
        .expect_err("nothing to call with the size");
    assert!(err.to_string().contains("or = fallback"), "{err}");
}

#[test]
fn an_unknown_key_is_an_error() {
    let err =
        expand_str("struct T { #[token(default)] pad_x: Option<f32> }").expect_err("not a key");
    assert!(err.to_string().contains("`or = fallback`"), "{err}");
}

#[test]
fn only_a_struct_of_named_fields_derives() {
    assert!(expand_str("enum T { A }").is_err());
    assert!(expand_str("struct T(Option<f32>);").is_err());
}
