use super::*;

fn write_index(dir: &Path, index: &CatalogIndex) {
    std::fs::create_dir_all(dir.join(".telar")).unwrap();
    std::fs::write(
        dir.join(".telar").join(CATALOG_INDEX_FILENAME),
        index.to_json(),
    )
    .unwrap();
}

fn probe(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rsx_catalog_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("locales")).unwrap();
    dir
}

fn index_for(dir: &Path, entries: Vec<CatalogEntry>) -> CatalogIndex {
    let mut sources = Vec::new();
    if let Ok(bytes) = std::fs::read(dir.join("locales/en.toml")) {
        sources.push(CatalogSourceFile {
            path: "locales/en.toml".to_string(),
            hash: content_hash(&bytes),
        });
    }
    CatalogIndex {
        format: CATALOG_ARTIFACT_FORMAT,
        producer: "test".to_string(),
        telar_version: "0.1.8".to_string(),
        entries,
        sources,
    }
}

#[test]
fn a_known_key_answers_with_its_arguments() {
    let dir = probe("known");
    write_index(
        &dir,
        &index_for(
            &dir,
            vec![CatalogEntry {
                key: "greeting".to_string(),
                args: vec!["name".to_string()],
            }],
        ),
    );

    let catalog = CatalogContext::load(&dir, "0.1.8");
    assert_eq!(
        catalog.arg_names("greeting").unwrap(),
        Some(["name".to_string()].as_slice())
    );
    assert_eq!(catalog.arg_names("missing").unwrap(), None);
    assert!(
        !catalog.is_empty(),
        "a baked catalog holds the keys it was built from"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// The distinction a missing file cannot make, and the reason an empty artifact is written at all: one is a project without translations, the other a build that never ran the baker.
#[test]
fn an_empty_catalog_is_not_a_missing_one() {
    let dir = probe("empty");
    write_index(&dir, &index_for(&dir, Vec::new()));

    let baked = CatalogContext::load(&dir, "0.1.8");
    assert!(baked.is_empty(), "this project declares no messages");
    assert_eq!(baked.arg_names("anything").unwrap(), None);

    let _ = std::fs::remove_dir_all(&dir);
    let unbaked = CatalogContext::load(&probe("unbaked"), "0.1.8");
    assert!(
        !unbaked.is_empty(),
        "an unbaked catalog is not the same as an empty one"
    );
    assert!(
        unbaked.arg_names("anything").is_err(),
        "and it cannot answer for a key it never loaded"
    );
}

#[test]
fn a_locale_edited_since_the_bake_is_reported_once_for_the_project() {
    let dir = probe("stale");
    std::fs::write(dir.join("locales/en.toml"), "greeting = \"Hi\"\n").unwrap();
    write_index(
        &dir,
        &index_for(
            &dir,
            vec![CatalogEntry {
                key: "greeting".to_string(),
                args: Vec::new(),
            }],
        ),
    );
    assert!(
        CatalogContext::load(&dir, "0.1.8").staleness().is_none(),
        "nothing was edited since the bake"
    );

    std::fs::write(dir.join("locales/en.toml"), "greeting = \"Hello\"\n").unwrap();
    let catalog = CatalogContext::load(&dir, "0.1.8");
    let stale = catalog.staleness().expect("the file changed");
    assert!(
        stale.contains("locales/en.toml") && stale.contains("cargo telar bake"),
        "{stale}"
    );
    assert!(
        catalog.arg_names("greeting").is_err(),
        "a stale catalog refuses to answer rather than guessing"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_artifact_baked_for_another_telar_names_both_commands() {
    let dir = probe("version");
    write_index(&dir, &index_for(&dir, Vec::new()));

    let err = CatalogContext::load(&dir, "9.9.9")
        .arg_names("anything")
        .unwrap_err();
    assert!(
        err.contains("cargo install cargo-telar --version 9.9.9"),
        "{err}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
