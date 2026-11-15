use super::*;

#[test]
fn a_catalog_decodes_under_the_locale_its_decoder_names() {
    let catalog = CatalogDecoder::new(telar::locale_tag!("es-ES"))
        .decode(b"greeting = \"Hola\"\n")
        .expect("valid catalog toml");
    assert_eq!(catalog.default_locale, telar::locale_tag!("es-ES"));
    assert!(
        catalog
            .message("greeting", &telar::locale_tag!("es-ES"))
            .is_some(),
        "the decoder's own locale is the one the catalog installs under"
    );
}

/// The property the whole path exists for: a body that does not parse never becomes a catalog, so it never reaches `AssetCache::put` either.
#[test]
fn a_body_that_is_not_toml_fails_rather_than_installing_an_empty_catalog() {
    assert!(
        CatalogDecoder::new(telar::locale_tag!("en"))
            .decode(b"<html>404 Not Found</html>")
            .is_err(),
        "a body that is not toml must fail rather than install an empty catalog"
    );
}
