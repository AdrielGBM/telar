//! Installing only into an empty slot, the way a crate wired by `rsx_modules!` installs its catalog as its binary loads. Its own binary because the installed catalog is process-wide.

use telar_i18n_core::{
    Catalog, Entry, Message, catalog, set_catalog, set_catalog_if_unset, set_locale, t,
};

static FIRST: Catalog = Catalog {
    locales: &["en"],
    default_locale: "en",
    entries: &[Entry {
        key: "greeting",
        messages: &[("en", Message::Plain("first"))],
    }],
};

static SECOND: Catalog = Catalog {
    locales: &["en"],
    default_locale: "en",
    entries: &[Entry {
        key: "greeting",
        messages: &[("en", Message::Plain("second"))],
    }],
};

#[test]
fn an_empty_slot_is_filled_and_an_occupied_one_is_kept_until_replaced() {
    set_locale("en");
    assert!(catalog().is_none());

    assert!(set_catalog_if_unset(&FIRST));
    assert_eq!(t("greeting", &[]), "first");

    assert!(!set_catalog_if_unset(&SECOND));
    assert_eq!(t("greeting", &[]), "first");

    set_catalog(&SECOND);
    assert_eq!(t("greeting", &[]), "second");
}
