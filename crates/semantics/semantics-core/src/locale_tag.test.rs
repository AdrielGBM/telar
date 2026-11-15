use super::*;

fn tag(text: &str) -> LocaleTag {
    LocaleTag::parse(text).unwrap()
}

#[test]
fn every_spelling_of_a_tag_is_one_tag() {
    assert_eq!(tag("en_us"), tag("en-US"));
    assert_eq!(tag("EN-US"), tag("en-US"));
    assert_eq!(tag("sr_latn_rs"), tag("sr-Latn-RS"));
    assert_ne!(tag("pt-BR"), tag("pt-PT"));
    assert_ne!(tag("pt"), tag("pt-BR"));
}

#[test]
fn a_tag_is_written_in_canonical_casing() {
    assert_eq!(tag("EN_us").to_string(), "en-US");
    assert_eq!(tag("SR-cyrl-rs").to_string(), "sr-Cyrl-RS");
    assert_eq!(tag("es-419").to_string(), "es-419");
}

#[test]
fn its_subtags_read_back_canonical() {
    let serbian = tag("sr_cyrl_rs");
    assert_eq!(serbian.language(), "sr");
    assert_eq!(serbian.script(), Some("Cyrl"));
    assert_eq!(serbian.region(), Some("RS"));
    assert_eq!(tag("he-IL").script(), None);
}

#[test]
fn extensions_are_read_past() {
    assert_eq!(tag("en-US-u-ca-buddhist"), tag("en-US"));
    assert_eq!(tag("en-US-x-posix"), tag("en-US"));
}

#[test]
fn text_that_is_no_tag_is_refused_naming_it() {
    for text in [
        "",
        "-x",
        "en-",
        "english language",
        "e",
        "en--US",
        "C.UTF-8",
    ] {
        let error = LocaleTag::parse(text).unwrap_err();
        assert_eq!(error.tag(), text);
        assert!(error.to_string().contains(&format!("`{text}`")));
    }
}

#[test]
fn a_tag_in_source_is_the_parsed_one() {
    static CHILEAN: LocaleTag = locale_tag!("es-cl");
    assert_eq!(CHILEAN, tag("es_CL"));
}

#[test]
fn tags_sort_as_their_spellings_do() {
    let mut tags = [
        tag("zh-Hant"),
        tag("ar-SA"),
        tag("ar-Latn"),
        tag("und-fonipa"),
    ];
    tags.sort();
    let spelled: Vec<String> = tags.iter().map(ToString::to_string).collect();
    assert_eq!(spelled, ["ar-Latn", "ar-SA", "und-fonipa", "zh-Hant"]);
}
