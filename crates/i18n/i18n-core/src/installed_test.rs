use super::*;
use crate::message::{Entry, Message, Part};

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
fn an_installed_catalog_follows_the_active_locale() {
    set_catalog(&CATALOG);
    crate::set_locale("es");
    assert_eq!(t("greeting", &[("name", "Ada")]), "Hola, Ada!");
    crate::set_locale("en");
    assert_eq!(t("greeting", &[("name", "Ada")]), "Hello, Ada!");
}

/// A missing key must reach the screen as itself. Returning an empty string would hide the gap, which is the one thing a translation lookup must not do.
#[test]
fn an_absent_key_renders_as_the_key() {
    set_catalog(&CATALOG);
    assert_eq!(t("nope", &[]), "nope");
}
