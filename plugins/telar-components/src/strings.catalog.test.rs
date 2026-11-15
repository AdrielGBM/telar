use telar::LocaleTag;

use super::*;

#[test]
fn the_entries_are_sorted_so_every_one_is_found() {
    CATALOG.assert_sorted();
}

#[test]
fn every_entry_speaks_every_locale_the_catalog_ships() {
    for entry in CATALOG.entries {
        let locales: Vec<&LocaleTag> = entry.messages.iter().map(|(locale, _)| locale).collect();
        let shipped: Vec<&LocaleTag> = CATALOG.locales.iter().collect();
        assert_eq!(locales, shipped, "`{}`", entry.key);
    }
}

#[test]
fn a_column_s_splitter_is_named_for_its_column_in_each_locale() {
    for (locale, expected) in [
        (telar::locale_tag!("en"), "Resize Size column"),
        (
            telar::locale_tag!("es"),
            "Cambiar el ancho de la columna Size",
        ),
        (telar::locale_tag!("ar"), "تغيير عرض العمود Size"),
    ] {
        telar::set_locale(locale);
        assert_eq!(text_with(RESIZE_COLUMN, &[("label", "Size")]), expected);
    }
    telar::set_locale(telar::locale_tag!("en"));
}
