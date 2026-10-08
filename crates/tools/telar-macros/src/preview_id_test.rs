use super::*;

fn expand_str(source: &str) -> syn::Result<String> {
    Ok(expand(syn::parse_str(source)?)?.to_string())
}

#[test]
fn the_id_is_the_crate_the_tag_and_the_slugged_name() {
    let out = expand_str(r#"button, "Primary — large""#).expect("a tag and a name");
    assert_eq!(
        out,
        r#"(:: core :: concat ! (:: core :: env ! ("CARGO_CRATE_NAME") , "--button--primary-large") , "button")"#
    );
}

#[test]
fn a_raw_tag_is_named_without_its_prefix_in_the_id_and_the_component() {
    let out = expand_str(r#"r#type, "Default""#).unwrap();
    assert!(out.contains(r#""--type--default""#), "{out}");
    assert!(out.ends_with(r#", "type")"#), "{out}");
}

#[test]
fn a_trailing_comma_is_accepted() {
    assert!(expand_str(r#"button, "Primary","#).is_ok());
}

#[test]
fn a_name_with_nothing_to_slug_is_refused() {
    let err = expand_str(r#"button, "—""#).expect_err("no letter or digit");
    assert!(err.to_string().contains("needs a letter or digit"), "{err}");
}

#[test]
fn a_name_that_is_not_a_string_is_refused() {
    assert!(expand_str("button, 42").is_err());
}
