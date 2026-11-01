use super::*;

static CATALOG: Catalog = Catalog {
    locales: &["en", "es"],
    default_locale: "en",
    entries: &[
        Entry {
            key: "battery.remaining",
            messages: &[
                (
                    "en",
                    Message::Format(&[Part::Arg("time"), Part::Lit(" remaining")]),
                ),
                (
                    "es",
                    Message::Format(&[Part::Lit("quedan "), Part::Arg("time")]),
                ),
            ],
        },
        Entry {
            key: "settings.title",
            messages: &[
                ("en", Message::Plain("Settings")),
                ("es", Message::Plain("Ajustes")),
            ],
        },
    ],
};

#[test]
fn plain_lookup_and_fallback() {
    assert_eq!(
        CATALOG.message("settings.title", "es").unwrap().render(&[]),
        "Ajustes"
    );
    assert_eq!(
        CATALOG.message("settings.title", "fr").unwrap().render(&[]),
        "Settings"
    );
    assert!(
        CATALOG.message("missing.key", "en").is_none(),
        "a key the catalog does not hold has no message"
    );
}

#[test]
fn named_args_substitute_and_reorder() {
    let en = CATALOG.message("battery.remaining", "en").unwrap();
    let es = CATALOG.message("battery.remaining", "es").unwrap();
    assert_eq!(en.render(&[("time", "5m")]), "5m remaining");
    assert_eq!(es.render(&[("time", "5m")]), "quedan 5m");
}

#[test]
fn missing_arg_stays_visible() {
    let en = CATALOG.message("battery.remaining", "en").unwrap();
    assert_eq!(en.render(&[]), "{time} remaining");
}

#[test]
fn arg_names_lists_placeholders() {
    assert_eq!(
        CATALOG
            .message("battery.remaining", "en")
            .unwrap()
            .arg_names(),
        vec!["time"]
    );
    assert!(
        CATALOG
            .message("settings.title", "en")
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
    Catalog {
        locales: &["en"],
        default_locale: "en",
        entries: &[
            Entry {
                key: "b",
                messages: &[("en", Message::Plain("B"))],
            },
            Entry {
                key: "a",
                messages: &[("en", Message::Plain("A"))],
            },
        ],
    }
    .assert_sorted();
}

#[test]
#[should_panic(expected = "catalog keys `a` and `a` are out of order or repeated")]
fn a_repeated_key_is_named() {
    Catalog {
        locales: &["en"],
        default_locale: "en",
        entries: &[
            Entry {
                key: "a",
                messages: &[("en", Message::Plain("A"))],
            },
            Entry {
                key: "a",
                messages: &[("en", Message::Plain("A"))],
            },
        ],
    }
    .assert_sorted();
}

#[test]
#[should_panic(expected = "the locales `es` and `en` of `a` are out of order or repeated")]
fn a_locale_out_of_order_is_named() {
    Catalog {
        locales: &["en", "es"],
        default_locale: "en",
        entries: &[Entry {
            key: "a",
            messages: &[("es", Message::Plain("A")), ("en", Message::Plain("A"))],
        }],
    }
    .assert_sorted();
}
