use telar_i18n_core::{Catalog, Entry, Message, Part, set_locale, translate_with_override};
use telar_i18n_core::{LocaleTag, locale_tag};

static PLUGIN: Catalog = Catalog {
    locales: &[locale_tag!("en"), locale_tag!("es")],
    default_locale: locale_tag!("en"),
    entries: &[
        Entry {
            key: "close",
            messages: &[
                (locale_tag!("en"), Message::Plain("Close")),
                (locale_tag!("es"), Message::Plain("Cerrar")),
            ],
        },
        Entry {
            key: "hello",
            messages: &[(
                locale_tag!("en"),
                Message::Format(&[Part::Lit("Hello, "), Part::Arg("name")]),
            )],
        },
    ],
};

#[test]
fn without_an_installed_catalog_the_plugin_text_is_used() {
    set_locale(tag("es"));
    assert_eq!(
        translate_with_override("ui", &PLUGIN, "close", &[]),
        "Cerrar"
    );
    set_locale(tag("en"));
    assert_eq!(
        translate_with_override("ui", &PLUGIN, "hello", &[("name", "Ada")]),
        "Hello, Ada"
    );
    assert_eq!(translate_with_override("ui", &PLUGIN, "nope", &[]), "nope");
}

fn tag(text: &str) -> LocaleTag {
    LocaleTag::parse(text).unwrap()
}
