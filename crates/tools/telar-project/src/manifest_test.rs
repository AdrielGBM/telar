use super::*;

fn package(name: &str, manifest: Option<&str>) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_manifest_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    if let Some(content) = manifest {
        std::fs::write(root.join(MANIFEST_FILENAME), content).unwrap();
    }
    root
}

#[test]
fn a_package_with_no_manifest_gets_the_defaults() {
    let root = package("absent", None);
    let manifest = TelarManifest::load(&root).expect("no file is not an error");
    assert_eq!(manifest, TelarManifest::default());
    assert_eq!(manifest.telar.assets_root(&root), root.join("assets"));
}

#[test]
fn every_key_the_schema_names_round_trips() {
    let root = package(
        "full",
        Some(
            r#"
[telar]
backend = "software"
assets = "art"
theme = "app::Theme"

[telar.dev]
devtools = false

[telar.dev.window]
title = "dev"
width = 1024

[telar.i18n]
root = "strings"
scan = "lang"
default = "es"
"#,
        ),
    );
    let telar = TelarManifest::load(&root)
        .expect("a full manifest parses")
        .telar;
    assert_eq!(telar.backend, Some(RendererBackend::Software));
    assert_eq!(telar.assets_root(&root), root.join("art"));
    assert_eq!(telar.theme.as_deref(), Some("app::Theme"));
    assert_eq!(telar.dev.devtools, Some(false));
    assert_eq!(telar.dev.window.as_ref().unwrap().width, Some(1024));
    assert_eq!(telar.locales_root(&root), Some(root.join("strings")));
    assert_eq!(telar.catalog_scan_dir(), "lang");
    assert_eq!(telar.default_locale().as_deref(), Some("es"));
}

/// The whole reason this file exists. Every reader treated an unrecognised key as absent, so a setting that was off and a setting that was misspelled looked identical — and `cargo-telar`'s own comment records the release where that shipped.
#[test]
fn a_misspelled_key_is_an_error_that_names_it() {
    for (label, manifest) in [
        ("top level", "[telar]\nbackends = \"software\"\n"),
        ("dev", "[telar.dev]\ndevtool = true\n"),
        ("window", "[telar.dev.window]\nwith = 800\n"),
        ("i18n", "[telar.i18n]\nscann = \"lang\"\n"),
        ("table", "[telarr]\nbackend = \"software\"\n"),
    ] {
        let root = package(&format!("typo_{}", label.replace(' ', "_")), Some(manifest));
        let error = TelarManifest::load(&root)
            .expect_err(&format!("a typo in the {label} table has to be an error"));
        let text = error.to_string();
        let key = manifest
            .lines()
            .find(|line| line.contains('='))
            .and_then(|line| line.split('=').next())
            .map(str::trim)
            .unwrap_or_default();
        let named = text.contains(key) || text.contains("telarr");
        assert!(
            named,
            "the {label} error should name the key it refused: {text}"
        );
    }
}

/// A value of the right shape but the wrong word is caught too, so `backend = "gpu"` is not silently `auto`.
#[test]
fn an_unknown_backend_is_refused_rather_than_defaulted() {
    let root = package("backend", Some("[telar]\nbackend = \"gpu\"\n"));
    assert!(TelarManifest::load(&root).is_err());
}

/// The pre-`[telar.i18n]` spellings still answer, so a project written against the old keys keeps working.
#[test]
fn the_older_i18n_spellings_still_answer() {
    let root = package(
        "legacy",
        Some("[telar]\nlocales = \"strings\"\ndefault_locale = \"fr\"\n"),
    );
    let telar = TelarManifest::load(&root)
        .expect("the old keys are still in the schema")
        .telar;
    assert_eq!(telar.locales_root(&root), Some(root.join("strings")));
    assert_eq!(telar.default_locale().as_deref(), Some("fr"));
}

/// `""` is how a project turns a discovery source off, which is different from leaving it at its default.
#[test]
fn an_empty_locales_root_disables_it() {
    let root = package("disabled", Some("[telar.i18n]\nroot = \"\"\n"));
    let telar = TelarManifest::load(&root).unwrap().telar;
    assert_eq!(telar.locales_root(&root), None);
}

/// A workspace declares once what its packages share. Every package answering the same way is the common case, and saying it in eight files is how the eight drift apart.
#[test]
fn a_package_inherits_the_workspace_manifest_key_by_key() {
    let root = std::env::temp_dir().join(format!("telar_inherit_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let package_root = root.join("crates/ui");
    std::fs::create_dir_all(&package_root).unwrap();
    std::fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    std::fs::write(
        root.join(MANIFEST_FILENAME),
        "[telar]\nbackend = \"software\"\ntheme = \"config::Nord\"\n",
    )
    .unwrap();
    std::fs::write(
        package_root.join(MANIFEST_FILENAME),
        "[telar]\ntheme = \"ui::Own\"\n",
    )
    .unwrap();

    let telar = TelarManifest::load(&package_root)
        .expect("both manifests parse")
        .telar;
    assert_eq!(telar.backend, Some(RendererBackend::Software), "inherited");
    assert_eq!(telar.theme.as_deref(), Some("ui::Own"), "overridden");

    std::fs::remove_file(package_root.join(MANIFEST_FILENAME)).unwrap();
    let telar = TelarManifest::load(&package_root)
        .expect("a package with no manifest of its own")
        .telar;
    assert_eq!(telar.theme.as_deref(), Some("config::Nord"));
    let _ = std::fs::remove_dir_all(&root);
}
