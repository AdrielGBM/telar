use std::collections::BTreeMap;

use super::*;

fn table(src: &str) -> toml::Table {
    src.parse().unwrap()
}

fn flat(src: &str) -> BTreeMap<String, MessageModel> {
    let mut out = BTreeMap::new();
    i18n_core::flatten(&table(src), String::new(), &mut out, "t.toml").unwrap();
    out
}

#[test]
fn plural_serializes_with_its_categories() {
    let out = flat("[items]\none = \"item\"\nother = \"items\"\n");
    let src = ser_message(&out["items"]);
    assert!(src.starts_with("Message::Plural(&["), "{src}");
    assert!(src.contains("PluralCategory::One"), "{src}");
    assert!(src.contains("PluralCategory::Other"), "{src}");
}

#[test]
fn the_plural_category_import_is_emitted_only_when_used() {
    let plain = CatalogModel {
        locales: vec!["en".into()],
        default_locale: "en".into(),
        entries: BTreeMap::from([(
            "a".to_string(),
            BTreeMap::from([("en".to_string(), MessageModel::Plain("x".into()))]),
        )]),
    };
    assert!(
        !to_source(&plain).contains("PluralCategory"),
        "a catalog with no plurals must not import the category:\n{}",
        to_source(&plain)
    );

    let mut with_plural = plain;
    with_plural.entries.insert(
        "items".to_string(),
        BTreeMap::from([(
            "en".to_string(),
            MessageModel::Plural(BTreeMap::from([(
                "other".to_string(),
                MessageModel::Plain("items".into()),
            )])),
        )]),
    );
    assert!(
        to_source(&with_plural).contains("PluralCategory"),
        "and one with plurals must:\n{}",
        to_source(&with_plural)
    );
}
