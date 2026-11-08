use super::*;

fn expand_str(source: &str) -> syn::Result<String> {
    Ok(expand(syn::parse_str::<DeriveInput>(source)?)?.to_string())
}

#[test]
fn the_impl_is_gated_behind_the_previews_feature() {
    let out = expand_str("enum Size { Small, Large }").expect("a fieldless enum derives");
    assert!(
        out.starts_with(":: telar :: __previews !"),
        "a build without previews has no trait to implement: {out}"
    );
    assert!(
        out.contains(":: telar :: preview :: PreviewArg for Size"),
        "{out}"
    );
}

#[test]
fn the_variants_are_listed_in_declaration_order() {
    let out = expand_str("enum Size { Small, Medium, Large }").unwrap();
    assert!(
        out.contains("variants : & [\"Small\" , \"Medium\" , \"Large\"]"),
        "{out}"
    );
}

#[test]
fn a_raw_variant_is_named_without_its_prefix() {
    let out = expand_str("enum Kind { r#type, Other }").unwrap();
    assert!(out.contains("\"type\""), "{out}");
    assert!(!out.contains("\"r#type\""), "{out}");
}

#[test]
fn a_struct_is_refused() {
    let err = expand_str("struct Size { small: bool }").expect_err("not an enum");
    assert!(err.to_string().contains("only for an enum"), "{err}");
}

#[test]
fn a_variant_with_fields_is_refused() {
    let err =
        expand_str("enum Size { Small, Custom(f32) }").expect_err("a field has no option to list");
    assert!(err.to_string().contains("no fields"), "{err}");
}

#[test]
fn an_empty_enum_is_refused() {
    let err = expand_str("enum Never {}").expect_err("nothing to choose");
    assert!(err.to_string().contains("at least one variant"), "{err}");
}

/// `none` is the text form of an unset `Option`, so a variant spelled that way would read back as no variant at all.
#[test]
fn a_variant_named_none_is_refused() {
    let err =
        expand_str("enum Fill { none, Solid }").expect_err("collides with the unset text form");
    assert!(err.to_string().contains("`none`"), "{err}");
}
