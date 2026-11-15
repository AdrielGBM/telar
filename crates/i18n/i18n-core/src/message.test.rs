use super::*;
use crate::{LocaleTag, locale_tag};

static CATALOG: Catalog = Catalog {
    locales: &[locale_tag!("en"), locale_tag!("es")],
    default_locale: locale_tag!("en"),
    entries: &[
        Entry {
            key: "battery.remaining",
            messages: &[
                (
                    locale_tag!("en"),
                    Message::Format(&[Part::Arg("time"), Part::Lit(" remaining")]),
                ),
                (
                    locale_tag!("es"),
                    Message::Format(&[Part::Lit("quedan "), Part::Arg("time")]),
                ),
            ],
        },
        Entry {
            key: "settings.title",
            messages: &[
                (locale_tag!("en"), Message::Plain("Settings")),
                (locale_tag!("es"), Message::Plain("Ajustes")),
            ],
        },
    ],
};

#[test]
fn plain_lookup_and_fallback() {
    assert_eq!(
        CATALOG
            .message("settings.title", &tag("es"))
            .unwrap()
            .render(&[]),
        "Ajustes"
    );
    assert_eq!(
        CATALOG
            .message("settings.title", &tag("fr"))
            .unwrap()
            .render(&[]),
        "Settings"
    );
    assert!(
        CATALOG.message("missing.key", &tag("en")).is_none(),
        "a key the catalog does not hold has no message"
    );
}

#[test]
fn named_args_substitute_and_reorder() {
    let en = CATALOG.message("battery.remaining", &tag("en")).unwrap();
    let es = CATALOG.message("battery.remaining", &tag("es")).unwrap();
    assert_eq!(en.render(&[("time", "5m")]), "5m remaining");
    assert_eq!(es.render(&[("time", "5m")]), "quedan 5m");
}

#[test]
fn missing_arg_stays_visible() {
    let en = CATALOG.message("battery.remaining", &tag("en")).unwrap();
    assert_eq!(en.render(&[]), "{time} remaining");
}

#[test]
fn arg_names_lists_placeholders() {
    assert_eq!(
        CATALOG
            .message("battery.remaining", &tag("en"))
            .unwrap()
            .arg_names(),
        vec!["time"]
    );
    assert!(
        CATALOG
            .message("settings.title", &tag("en"))
            .unwrap()
            .arg_names()
            .is_empty(),
        "every placeholder in the message must be listed"
    );
}

#[test]
fn a_sorted_catalog_passes_the_assertion() {
    CATALOG.assert_sorted();
}

#[test]
#[should_panic(expected = "catalog keys `b` and `a` are out of order or repeated")]
fn a_key_out_of_order_is_named() {
    static CATALOG: Catalog = Catalog {
        locales: &[locale_tag!("en")],
        default_locale: locale_tag!("en"),
        entries: &[
            Entry {
                key: "b",
                messages: &[(locale_tag!("en"), Message::Plain("B"))],
            },
            Entry {
                key: "a",
                messages: &[(locale_tag!("en"), Message::Plain("A"))],
            },
        ],
    };
    CATALOG.assert_sorted();
}

#[test]
#[should_panic(expected = "catalog keys `a` and `a` are out of order or repeated")]
fn a_repeated_key_is_named() {
    static CATALOG: Catalog = Catalog {
        locales: &[locale_tag!("en")],
        default_locale: locale_tag!("en"),
        entries: &[
            Entry {
                key: "a",
                messages: &[(locale_tag!("en"), Message::Plain("A"))],
            },
            Entry {
                key: "a",
                messages: &[(locale_tag!("en"), Message::Plain("A"))],
            },
        ],
    };
    CATALOG.assert_sorted();
}

#[test]
#[should_panic(expected = "the locale `es` of `a` is repeated")]
fn a_repeated_locale_is_named() {
    static CATALOG: Catalog = Catalog {
        locales: &[locale_tag!("en"), locale_tag!("es")],
        default_locale: locale_tag!("en"),
        entries: &[Entry {
            key: "a",
            messages: &[
                (locale_tag!("es"), Message::Plain("A")),
                (locale_tag!("ES"), Message::Plain("B")),
            ],
        }],
    };
    CATALOG.assert_sorted();
}

fn tag(text: &str) -> LocaleTag {
    LocaleTag::parse(text).unwrap()
}
