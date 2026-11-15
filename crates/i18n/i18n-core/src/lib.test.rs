use super::*;
use crate::{LocaleTag, locale_tag};

static CATALOG: Catalog = Catalog {
    locales: &[locale_tag!("en"), locale_tag!("es")],
    default_locale: locale_tag!("en"),
    entries: &[Entry {
        key: "greeting",
        messages: &[
            (
                locale_tag!("en"),
                Message::Format(&[Part::Lit("Hello, "), Part::Arg("name"), Part::Lit("!")]),
            ),
            (
                locale_tag!("es"),
                Message::Format(&[Part::Lit("Hola, "), Part::Arg("name"), Part::Lit("!")]),
            ),
        ],
    }],
};

#[test]
fn translate_follows_active_locale() {
    set_locale(tag("en"));
    assert_eq!(
        translate(&CATALOG, "greeting", &[("name", "Ada")]),
        "Hello, Ada!"
    );
    set_locale(tag("es"));
    assert_eq!(
        translate(&CATALOG, "greeting", &[("name", "Ada")]),
        "Hola, Ada!"
    );
}

#[test]
fn translate_falls_back_to_key_when_absent() {
    set_locale(tag("en"));
    assert_eq!(translate(&CATALOG, "nope", &[]), "nope");
}

fn tag(text: &str) -> LocaleTag {
    LocaleTag::parse(text).unwrap()
}
