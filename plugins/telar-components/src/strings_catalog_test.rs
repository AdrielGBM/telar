use super::*;

#[test]
fn the_entries_are_sorted_so_every_one_is_found() {
    CATALOG.assert_sorted();
}

#[test]
fn every_entry_speaks_every_locale_the_catalog_ships() {
    for entry in CATALOG.entries {
        let locales: Vec<&str> = entry.messages.iter().map(|(locale, _)| *locale).collect();
        assert_eq!(locales, CATALOG.locales, "`{}`", entry.key);
    }
}

#[test]
fn a_message_with_a_placeholder_is_filled_in_each_locale() {
    for (locale, expected) in [
        ("en", "Remove Rust"),
        ("es", "Quitar Rust"),
        ("ar", "إزالة Rust"),
    ] {
        telar::set_locale(locale);
        assert_eq!(text_with(REMOVE, &[("label", "Rust")]), expected);
    }
    telar::set_locale("en");
}
