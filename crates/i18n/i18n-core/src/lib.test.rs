use super::*;

static CATALOG: Catalog = Catalog {
    locales: &["en", "es"],
    default_locale: "en",
    entries: &[Entry {
        key: "greeting",
        messages: &[
            (
                "en",
                Message::Format(&[Part::Lit("Hello, "), Part::Arg("name"), Part::Lit("!")]),
            ),
            (
                "es",
                Message::Format(&[Part::Lit("Hola, "), Part::Arg("name"), Part::Lit("!")]),
            ),
        ],
    }],
};

#[test]
fn translate_follows_active_locale() {
    set_locale("en");
    assert_eq!(
        translate(&CATALOG, "greeting", &[("name", "Ada")]),
        "Hello, Ada!"
    );
    set_locale("es");
    assert_eq!(
        translate(&CATALOG, "greeting", &[("name", "Ada")]),
        "Hola, Ada!"
    );
}

#[test]
fn translate_falls_back_to_key_when_absent() {
    set_locale("en");
    assert_eq!(translate(&CATALOG, "nope", &[]), "nope");
}
