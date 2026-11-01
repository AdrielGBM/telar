use super::*;

fn flat(src: &str) -> BTreeMap<String, MessageModel> {
    let mut out = BTreeMap::new();
    let table: toml::Table = src.parse().unwrap();
    flatten(&table, String::new(), &mut out, "t.toml").unwrap();
    out
}

#[test]
fn a_category_table_becomes_one_plural_key() {
    let out = flat("[items]\none = \"{count} item\"\nother = \"{count} items\"\n");
    assert_eq!(out.len(), 1, "not two namespaced keys: {out:?}");
    let MessageModel::Plural(branches) = &out["items"] else {
        panic!("expected a plural, got {:?}", out["items"]);
    };
    assert_eq!(branches.len(), 2);
    assert!(
        branches.contains_key("one") && branches.contains_key("other"),
        "a category table becomes one key with its branches: {branches:?}"
    );
}

#[test]
fn a_namespace_table_is_still_flattened() {
    let out = flat("[nav]\noverview = \"Overview\"\nsettings = \"Settings\"\n");
    assert_eq!(out.len(), 2);
    assert!(
        out.contains_key("nav.overview") && out.contains_key("nav.settings"),
        "a namespace table flattens into dotted keys: {out:?}"
    );
}

#[test]
fn a_namespace_is_not_swallowed_just_because_one_key_looks_like_a_category() {
    // `other` alone is a plausible namespace entry; without the all-keys test this would misparse.
    let out = flat("[status]\none = \"Single\"\nname = \"Status\"\n");
    assert_eq!(out.len(), 2, "{out:?}");
    assert!(
        out.contains_key("status.one"),
        "a namespace whose key looks like a category is still a namespace: {out:?}"
    );

    // And a category table missing `other` is a namespace, since CLDR requires `other` of every language.
    let out = flat("[thing]\none = \"One\"\nfew = \"Few\"\n");
    assert!(out.contains_key("thing.one"), "{out:?}");
}

#[test]
fn plural_arg_names_union_the_branches() {
    let out = flat("[items]\none = \"one item\"\nother = \"{count} items\"\n");
    assert_eq!(out["items"].arg_names(), vec!["count".to_string()]);
}

#[test]
fn parses_plain_and_formatted_messages() {
    assert_eq!(
        parse_message("Settings"),
        MessageModel::Plain("Settings".into())
    );
    assert_eq!(
        parse_message("{time} remaining"),
        MessageModel::Format(vec![
            PartModel::Arg("time".into()),
            PartModel::Lit(" remaining".into()),
        ])
    );
    assert_eq!(
        parse_message("{{ {name} }}"),
        MessageModel::Format(vec![
            PartModel::Lit("{ ".into()),
            PartModel::Arg("name".into()),
            PartModel::Lit(" }".into()),
        ])
    );
}

/// The point of the whole module: what a runtime load produces must be indistinguishable from what the baker emits, down to the binary searches `Catalog::message` does over both.
#[test]
fn a_leaked_catalog_answers_like_a_baked_one() {
    let catalog = CatalogModel::from_sources(
        &[
            (
                "en",
                "title = \"Settings\"\n[battery]\nremaining = \"{time} remaining\"\n",
                "en",
            ),
            (
                "es",
                "title = \"Ajustes\"\n[battery]\nremaining = \"quedan {time}\"\n",
                "es",
            ),
        ],
        None,
    )
    .unwrap()
    .leak();

    assert_eq!(catalog.locales, ["en", "es"]);
    assert_eq!(catalog.default_locale, "en");
    assert_eq!(
        catalog
            .message("battery.remaining", "es")
            .unwrap()
            .render(&[("time", "5m")]),
        "quedan 5m"
    );
    assert_eq!(
        catalog.message("title", "fr").unwrap().render(&[]),
        "Settings"
    );
    assert!(
        catalog.message("absent", "en").is_none(),
        "a catalog answers nothing for a key it does not hold"
    );
}

#[test]
fn a_plural_table_survives_the_round_trip() {
    let catalog = CatalogModel::from_sources(
        &[(
            "en",
            "[items]\none = \"{count} item\"\nother = \"{count} items\"\n",
            "en",
        )],
        None,
    )
    .unwrap()
    .leak();
    let message = catalog.message("items", "en").unwrap();
    assert_eq!(
        message
            .select("en", &[("count", "1")])
            .render(&[("count", "1")]),
        "1 item"
    );
    assert_eq!(
        message
            .select("en", &[("count", "3")])
            .render(&[("count", "3")]),
        "3 items"
    );
}

#[test]
fn one_key_defined_twice_for_one_locale_is_an_error() {
    let err = CatalogModel::from_sources(
        &[
            ("en", "title = \"A\"\n", "a.toml"),
            ("en", "title = \"B\"\n", "b.toml"),
        ],
        None,
    )
    .unwrap_err();
    assert!(err.contains("duplicate key `title`"), "{err}");
}

#[test]
fn a_directory_of_locale_files_loads_as_one_catalog() {
    let dir = std::env::temp_dir().join(format!("telar_runtime_catalog_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("en.toml"), "greeting = \"Hello, {name}!\"\n").unwrap();
    std::fs::write(dir.join("es.toml"), "greeting = \"¡Hola, {name}!\"\n").unwrap();

    let catalog = Catalog::from_dir(&dir, Some("es")).unwrap();
    assert_eq!(catalog.default_locale, "es");
    assert_eq!(
        catalog
            .message("greeting", "en")
            .unwrap()
            .render(&[("name", "Ada")]),
        "Hello, Ada!"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
