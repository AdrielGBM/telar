use telar_i18n_core::{Catalog, Entry, Message, Part, set_locale, translate_with_override};

static PLUGIN: Catalog = Catalog {
    locales: &["en", "es"],
    default_locale: "en",
    entries: &[
        Entry {
            key: "close",
            messages: &[
                ("en", Message::Plain("Close")),
                ("es", Message::Plain("Cerrar")),
            ],
        },
        Entry {
            key: "hello",
            messages: &[(
                "en",
                Message::Format(&[Part::Lit("Hello, "), Part::Arg("name")]),
            )],
        },
    ],
};

#[test]
fn without_an_installed_catalog_the_plugin_text_is_used() {
    set_locale("es");
    assert_eq!(
        translate_with_override("ui", &PLUGIN, "close", &[]),
        "Cerrar"
    );
    set_locale("en");
    assert_eq!(
        translate_with_override("ui", &PLUGIN, "hello", &[("name", "Ada")]),
        "Hello, Ada"
    );
    assert_eq!(translate_with_override("ui", &PLUGIN, "nope", &[]), "nope");
}
