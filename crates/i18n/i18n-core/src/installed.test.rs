use super::*;
use crate::message::{Entry, Message, Part};
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
fn an_installed_catalog_follows_the_active_locale() {
    set_catalog(&CATALOG);
    crate::set_locale(tag("es"));
    assert_eq!(t("greeting", &[("name", "Ada")]), "Hola, Ada!");
    crate::set_locale(tag("en"));
    assert_eq!(t("greeting", &[("name", "Ada")]), "Hello, Ada!");
}

/// A missing key must reach the screen as itself. Returning an empty string would hide the gap, which is the one thing a translation lookup must not do.
#[test]
fn an_absent_key_renders_as_the_key() {
    set_catalog(&CATALOG);
    assert_eq!(t("nope", &[]), "nope");
}

#[test]
fn the_default_locale_is_the_catalogs_or_english_without_one() {
    static SPANISH_FIRST: Catalog = Catalog {
        locales: &[locale_tag!("es")],
        default_locale: locale_tag!("es"),
        entries: &[],
    };
    assert_eq!(*default_locale_of(Some(&SPANISH_FIRST)), locale_tag!("es"));
    assert_eq!(*default_locale_of(None), locale_tag!("en"));
}

fn tag(text: &str) -> LocaleTag {
    LocaleTag::parse(text).unwrap()
}
