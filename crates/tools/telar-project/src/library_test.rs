use super::*;

fn package(name: &str, files: &[&str]) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("telar_library_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for file in files {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let content = match *file {
            "src/lib.rs" => "telar::rsx_modules!();\n",
            _ => "x\n",
        };
        std::fs::write(path, content).unwrap();
    }
    root
}

/// Only what a dependency build reads: the Plain flavour, never the other three a workspace member also has on disk, and no record of a failed transpile.
#[test]
fn a_library_ships_its_sources_and_the_plain_artifact_only() {
    let root = package(
        "files",
        &[
            "telar.toml",
            "src/lib.rs",
            "src/util.rs",
            "src/home.rsx",
            "src/sub/card.rsx",
            "src/.telar/modules.rs",
            "src/.telar/modules-hot.rs",
            ".telar/build/home.rs",
            ".telar/build/home.rs.map",
            ".telar/build/sub/card.rs",
            ".telar/build/__modules/sub.rs",
            ".telar/build.json",
            ".telar/build.failure.json",
            ".telar/build-hot/home.rs",
            ".telar/build-hot.json",
            ".telar/i18n.rs",
            ".telar/i18n.json",
            ".telar/assets.rs",
            ".telar/assets.json",
            "locales/en.toml",
            "assets/logo.svg",
        ],
    );

    let files = library_files(&root);
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(
        files,
        [
            ".telar/assets.json",
            ".telar/assets.rs",
            ".telar/build.json",
            ".telar/build/__modules/sub.rs",
            ".telar/build/home.rs",
            ".telar/build/home.rs.map",
            ".telar/build/sub/card.rs",
            ".telar/i18n.json",
            ".telar/i18n.rs",
            "src/.telar/modules.rs",
            "src/home.rsx",
            "src/lib.rs",
            "src/sub/card.rsx",
            "src/util.rs",
            "telar.toml",
        ]
    );
}

#[test]
fn the_include_entries_cover_every_file_a_library_ships() {
    let root = package(
        "covered",
        &[
            "telar.toml",
            "src/lib.rs",
            "src/home.rsx",
            "src/.telar/modules.rs",
            ".telar/build/home.rs",
            ".telar/build/home.rs.map",
            ".telar/build/__modules/sub.rs",
            ".telar/build.json",
            ".telar/i18n.rs",
            ".telar/i18n.json",
            ".telar/assets.rs",
            ".telar/assets.json",
        ],
    );
    let files = library_files(&root);
    let _ = std::fs::remove_dir_all(&root);

    let include = library_include();
    for file in &files {
        assert!(
            include
                .iter()
                .any(|entry| library_include_covers(entry, file)),
            "{file} is in no include entry"
        );
    }
}

#[test]
fn an_include_entry_covers_a_directory_or_one_file() {
    assert!(library_include_covers("/src/**", "src/.telar/modules.rs"));
    assert!(library_include_covers(
        "/.telar/build/**",
        ".telar/build/a/b.rs"
    ));
    assert!(!library_include_covers(
        "/.telar/build/**",
        ".telar/build-hot/a.rs"
    ));
    assert!(!library_include_covers(
        "/.telar/build/**",
        ".telar/build.json"
    ));
    assert!(library_include_covers(
        "/.telar/build.json",
        ".telar/build.json"
    ));
    assert!(!library_include_covers("/telar.toml", "src/telar.toml"));
}

#[test]
fn a_negated_entry_covers_nothing() {
    assert!(!library_include_covers(
        "!/src/**/.telar/modules-hot.rs",
        "src/.telar/modules-hot.rs"
    ));
}

/// Every flavour but Plain is left out of `src/`, and nothing a dependency build reads is.
#[test]
fn the_include_leaves_out_the_module_trees_of_the_other_flavours() {
    let include = library_include();
    for flavour in BuildFlavour::ALL {
        let entry = format!("!/src/**/.telar/{}", flavour.site_file_name());
        assert_eq!(
            include.contains(&entry),
            flavour != BuildFlavour::Plain,
            "{entry}"
        );
    }
    let first_negation = include
        .iter()
        .position(|entry| entry.starts_with('!'))
        .unwrap();
    let src = include.iter().position(|entry| entry == "/src/**").unwrap();
    assert!(
        src < first_negation,
        "a negation only removes what an entry before it added"
    );
}
