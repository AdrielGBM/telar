use super::*;

const SQUARE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2 H22 V22 H2 Z"/></svg>"#;

#[test]
fn a_document_decodes_and_a_body_that_is_not_one_does_not() {
    assert!(
        SvgDecoder.decode(SQUARE.as_bytes()).is_ok(),
        "a real document decodes"
    );
    assert!(
        SvgDecoder.decode(b"<html>404 Not Found</html>").is_err(),
        "and an error page is not one"
    );
}

/// The kind namespaces the cache, so an SVG named `logo` and a bitmap named `logo` cannot share a file.
#[test]
fn the_kind_names_the_format_and_not_the_id() {
    assert_eq!(SvgDecoder.kind(), "svg");
}
