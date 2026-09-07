use super::*;

#[test]
fn telar_toml_overrides_manifest_field_by_field() {
    let manifest = TelarConfig {
        backend: Some(RendererBackend::Software),
        dev: Some(DevConfig {
            window: Some(WindowConfig {
                width: Some(800),
                height: Some(600),
                fullscreen: Some("disabled".to_string()),
                ..Default::default()
            }),
            devtools: Some(true),
        }),
    };
    let file = TelarConfig {
        backend: Some(RendererBackend::Hardware),
        dev: Some(DevConfig {
            window: Some(WindowConfig {
                width: Some(1024),
                position: Some("10,20".to_string()),
                ..Default::default()
            }),
            devtools: None,
        }),
    };

    let merged = merge_config(manifest, file);
    assert!(
        matches!(merged.backend, Some(RendererBackend::Hardware)),
        "telar.toml wins over the manifest"
    );
    let dev = merged.dev.unwrap();
    assert_eq!(dev.devtools, Some(true)); // omitted in telar.toml → manifest value survives
    let window = dev.window.unwrap();
    assert_eq!(window.width, Some(1024)); // telar.toml wins
    assert_eq!(window.height, Some(600)); // falls back to manifest
    assert_eq!(window.position.as_deref(), Some("10,20")); // only in telar.toml
    assert_eq!(window.fullscreen.as_deref(), Some("disabled")); // only in manifest
}

#[test]
fn telar_table_is_the_one_read_from_telar_toml() {
    let parsed: TelarToml =
        toml::from_str("[telar]\nbackend = \"software\"\nauto_modules = true\n")
            .expect("[telar] should parse, and `auto_modules` is the app macro's to read");
    assert!(
        matches!(parsed.telar.backend, Some(RendererBackend::Software)),
        "the [telar] table is what is read"
    );
}

// The rename that named this table `telar` left the field called `rsx`, and a `#[serde(default)]` table means a file writing `[telar]` still parsed clean — backend, dev window and devtools dropped in silence.
#[test]
fn a_top_level_table_that_is_not_telar_is_rejected_rather_than_ignored() {
    assert!(
        toml::from_str::<TelarToml>("[rsx]\nbackend = \"software\"\n").is_err(),
        "an unknown top-level table is a mistake worth reporting, not silence"
    );
}

#[test]
fn manifest_used_when_telar_toml_absent() {
    let manifest = TelarConfig {
        backend: Some(RendererBackend::Software),
        dev: None,
    };
    let merged = merge_config(manifest, TelarConfig::default());
    assert!(
        matches!(merged.backend, Some(RendererBackend::Software)),
        "with no telar.toml the manifest is the whole answer"
    );
    assert!(merged.dev.is_none(), "and it declared no dev section");
}

#[test]
fn workspace_inherited_version_is_recorded_as_inherited() {
    // `version = { workspace = true }` must not fail parsing; it records that the workspace holds the answer.
    let manifest: CargoManifest = toml::from_str(
        "[package]\nname = \"demo\"\nversion = { workspace = true }\ndescription = \"hi\"\n",
    )
    .expect("workspace-inherited version should parse");
    let pkg = manifest.package.expect("package section");
    assert_eq!(pkg.name, "demo");
    assert_eq!(pkg.version, Inheritable::FromWorkspace);
    assert_eq!(pkg.description, Inheritable::Set("hi".to_string()));
}

#[test]
fn crate_type_distinguishes_a_hot_reloadable_package() {
    let plain: CargoManifest =
        toml::from_str("[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[lib]\n")
            .expect("bare [lib] should parse");
    assert!(
        plain.lib.expect("lib section").crate_type.is_empty(),
        "a plain package declares no crate-type"
    );

    let dylib: CargoManifest = toml::from_str(
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n\n[lib]\ncrate-type = [\"cdylib\", \"lib\"]\n",
    )
    .expect("crate-type should parse");
    assert!(
        dylib
            .lib
            .expect("lib section")
            .crate_type
            .iter()
            .any(|kind| kind == "cdylib"),
        "a cdylib package is the hot-reloadable one"
    );
}

#[test]
fn workspace_inherited_authors_are_recorded_as_inherited() {
    let manifest: CargoManifest =
        toml::from_str("[package]\nname = \"demo\"\nauthors = { workspace = true }\n")
            .expect("workspace-inherited authors should parse");
    let pkg = manifest.package.expect("package section");
    assert_eq!(pkg.authors, Inheritable::FromWorkspace);
}

#[test]
fn the_dotted_spelling_of_inheritance_is_read_the_same_way() {
    // `version.workspace = true` is the form cargo's own docs use, and the one hyprshell writes.
    let manifest: CargoManifest =
        toml::from_str("[package]\nname = \"demo\"\nversion.workspace = true\n")
            .expect("dotted inheritance should parse");
    let pkg = manifest.package.expect("package section");
    assert_eq!(pkg.version, Inheritable::FromWorkspace);
}

#[test]
fn explicit_authors_list_deserializes() {
    let manifest: CargoManifest = toml::from_str(
        "[package]\nname = \"demo\"\nauthors = [\"Ada <ada@example.org>\", \"Bo <bo@example.org>\"]\n",
    )
    .expect("explicit authors should parse");
    let pkg = manifest.package.expect("package section");
    assert_eq!(
        pkg.authors,
        Inheritable::Set(vec![
            "Ada <ada@example.org>".to_string(),
            "Bo <bo@example.org>".to_string(),
        ])
    );
}

#[test]
fn explicit_version_string_deserializes() {
    let manifest: CargoManifest =
        toml::from_str("[package]\nname = \"demo\"\nversion = \"2.0.1\"\n")
            .expect("explicit version should parse");
    let pkg = manifest.package.expect("package section");
    assert_eq!(pkg.version, Inheritable::Set("2.0.1".to_string()));
}

#[test]
fn an_inherited_field_is_answered_by_the_workspace_not_the_fallback() {
    // Regression: a member inheriting its version reported the hardcoded "0.1.0", so every bundle carried the wrong version the moment the workspace moved off it.
    let manifest: CargoManifest = toml::from_str(
        "[package]\nname = \"demo\"\nversion.workspace = true\nauthors.workspace = true\n",
    )
    .expect("inherited fields should parse");
    let resolved = ResolvedPackage {
        workspace_root: PathBuf::from("/nowhere"),
        package: manifest.package,
        workspace_package: Some(CargoWorkspacePackage {
            version: Some("2.0.0".to_string()),
            authors: vec!["Ada <ada@example.org>".to_string()],
            description: None,
        }),
        produces_cdylib: false,
        features: Default::default(),
    };
    assert_eq!(resolved.version(), "2.0.0");
    assert_eq!(
        resolved.maintainer().as_deref(),
        Some("Ada <ada@example.org>")
    );
}

#[test]
fn the_debian_environment_names_a_maintainer_when_the_manifest_does_not() {
    assert_eq!(
        maintainer_from(Some("Ada".into()), Some("ada@example.org".into())).as_deref(),
        Some("Ada <ada@example.org>")
    );
    // DEBEMAIL alone is a complete answer: dpkg accepts a bare address as the maintainer.
    assert_eq!(
        maintainer_from(None, Some("ada@example.org".into())).as_deref(),
        Some("ada@example.org")
    );
    assert_eq!(maintainer_from(Some("Ada".into()), None), None);
    assert_eq!(maintainer_from(None, Some("   ".into())), None);
}

#[test]
fn an_unreadable_manifest_still_falls_back() {
    let resolved = ResolvedPackage {
        workspace_root: PathBuf::from("/nowhere"),
        package: None,
        workspace_package: None,
        produces_cdylib: false,
        features: Default::default(),
    };
    assert_eq!(resolved.name(), "app");
    assert_eq!(resolved.version(), "0.1.0");
    assert_eq!(resolved.maintainer(), None);
}

/// The failure this exists to explain: an installed CLI that is not the project's produces an artifact the project's macro may refuse, and the refusal names a command without naming the reason.
#[test]
fn a_project_on_another_telar_is_named() {
    let note = foreign_version_note("0.1.7").expect("0.1.7 is not this binary's version");

    assert!(note.contains("0.1.7"), "{note}");
    assert!(note.contains(env!("CARGO_PKG_VERSION")), "{note}");
    assert!(note.contains("cargo install cargo-telar"), "{note}");
}

/// The common case says nothing: most builds are run by the CLI the project was written against.
#[test]
fn the_matching_version_says_nothing() {
    assert_eq!(foreign_version_note(env!("CARGO_PKG_VERSION")), None);
}
