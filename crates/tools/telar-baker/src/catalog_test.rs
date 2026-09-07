use super::*;

#[test]
fn bakes_catalog_from_disk() {
    let root = std::env::temp_dir().join(format!("rsx_i18n_bake_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("locales")).unwrap();
    std::fs::write(
        root.join("locales/en.toml"),
        "title = \"Settings\"\n[battery]\nremaining = \"{time} remaining\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("locales/es.toml"),
        "title = \"Ajustes\"\n[battery]\nremaining = \"quedan {time}\"\n",
    )
    .unwrap();

    let model = parse_catalog(&root).unwrap().unwrap();
    assert_eq!(model.locales, vec!["en", "es"]);
    assert_eq!(model.default_locale, "en");
    assert!(
        model.contains_key("battery.remaining"),
        "the parsed model is missing a key: {model:?}"
    );
    assert_eq!(model.arg_names("battery.remaining").unwrap(), vec!["time"]);

    let src = to_source(&model);
    assert!(src.contains("default_locale: \"en\""), "{src}");
    assert!(src.contains("Entry { key: \"battery.remaining\""), "{src}");
    assert!(src.contains("Message::Plain(\"Ajustes\")"), "{src}");
    assert!(src.contains("Part::Arg(\"time\")"), "{src}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn merges_per_module_files_in_a_locale_dir() {
    let root = std::env::temp_dir().join(format!("rsx_i18n_multi_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("locales/en")).unwrap();
    std::fs::write(
        root.join("locales/en/settings.toml"),
        "[settings]\ntitle = \"Settings\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("locales/en/battery.toml"),
        "[battery]\nfull = \"Full\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("locales/es.toml"),
        "[settings]\ntitle = \"Ajustes\"\n[battery]\nfull = \"Llena\"\n",
    )
    .unwrap();

    let model = parse_catalog(&root).unwrap().unwrap();
    assert_eq!(model.locales, vec!["en", "es"]);
    assert!(
        model.contains_key("settings.title"),
        "a per-module file was dropped in the merge: {model:?}"
    );
    assert!(
        model.contains_key("battery.full"),
        "the other module was dropped in the merge: {model:?}"
    );
    assert!(
        catalog_files(&root).len() == 3,
        "every module file in the locale dir counts: {:?}",
        catalog_files(&root)
    );

    std::fs::write(
        root.join("locales/en/dup.toml"),
        "[settings]\ntitle = \"Oops\"\n",
    )
    .unwrap();
    let err = parse_catalog(&root).unwrap_err();
    assert!(err.contains("duplicate key `settings.title`"), "{err}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn discovers_co_located_module_catalogs() {
    let root = std::env::temp_dir().join(format!("rsx_i18n_colo_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src/modules/battery/i18n")).unwrap();
    std::fs::create_dir_all(root.join("src/modules/settings/i18n")).unwrap();
    std::fs::create_dir_all(root.join("locales")).unwrap();
    std::fs::write(
        root.join("src/modules/battery/i18n/en.toml"),
        "[battery]\nfull = \"Full\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/modules/battery/i18n/es.toml"),
        "[battery]\nfull = \"Llena\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/modules/settings/i18n/en.toml"),
        "[settings]\ntitle = \"Settings\"\n",
    )
    .unwrap();
    std::fs::write(root.join("locales/en.toml"), "[common]\non = \"On\"\n").unwrap();

    let model = parse_catalog(&root).unwrap().unwrap();
    assert_eq!(model.locales, vec!["en", "es"]);
    assert!(
        model.contains_key("battery.full"),
        "a co-located module catalog was missed: {model:?}"
    );
    assert!(
        model.contains_key("settings.title"),
        "the other one was missed too: {model:?}"
    );
    assert!(
        model.contains_key("common.on"),
        "project-wide locales/ still merges alongside the scan"
    );

    std::fs::write(root.join("locales/es.toml"), "[common]\non = \"Sí\"\n").unwrap();
    std::fs::write(
        root.join("telar.toml"),
        "[telar.i18n]\nscan = \"translations\"\ndefault = \"es\"\n",
    )
    .unwrap();
    let model2 = parse_catalog(&root).unwrap().unwrap();
    assert!(
        !model2.contains_key("battery.full"),
        "the renamed scan dir no longer matches i18n/"
    );
    assert_eq!(model2.default_locale, "es");
    assert!(
        model2.contains_key("common.on"),
        "the project-wide root is unaffected by the scan name"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn no_locales_dir_is_none() {
    let root = std::env::temp_dir().join(format!("rsx_i18n_none_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    assert!(
        parse_catalog(&root).unwrap().is_none(),
        "no locales dir means no catalog, not an empty one"
    );
    let _ = std::fs::remove_dir_all(&root);
}
