use super::*;

fn read(tag: &str) -> (&str, Option<&str>) {
    let parsed = LocaleTag::parse(tag);
    (parsed.language, parsed.script)
}

#[test]
fn a_bare_language_has_no_script() {
    assert_eq!(read("en"), ("en", None));
    assert_eq!(read("es-CL"), ("es", None));
    assert_eq!(read("es-419"), ("es", None));
}

#[test]
fn a_script_is_read_after_either_separator() {
    assert_eq!(read("sr-Cyrl"), ("sr", Some("Cyrl")));
    assert_eq!(read("sr_Cyrl_RS"), ("sr", Some("Cyrl")));
    assert_eq!(read("he_IL"), ("he", None));
}

#[test]
fn extended_language_subtags_come_before_the_script() {
    assert_eq!(read("zh-yue-Hant-HK"), ("zh", Some("Hant")));
}

#[test]
fn a_four_letter_subtag_after_the_region_is_not_a_script() {
    assert_eq!(read("de-DE-1996"), ("de", None));
    assert_eq!(read("en-US-x-abcd"), ("en", None));
}

#[test]
fn an_empty_or_leading_separator_tag_has_an_empty_language() {
    assert_eq!(read(""), ("", None));
    assert_eq!(read("-x"), ("", None));
}

#[test]
fn tags_are_the_same_whatever_their_case_and_separators() {
    assert!(same_tag("pt_BR", "PT-br"));
    assert!(same_tag("sr-Latn-RS", "sr_latn_rs"));
    assert!(!same_tag("pt-BR", "pt-PT"));
    assert!(!same_tag("pt", "pt-BR"));
    assert!(!same_tag("ptxBR", "pt-BR"));
}
