use super::*;

#[test]
fn telar_toml_overrides_manifest_field_by_field() {
    let manifest = TelarSection {
        backend: Some(RendererBackend::Software),
        dev: DevSection {
            window: Some(WindowSection {
                width: Some(800),
                height: Some(600),
                fullscreen: Some("disabled".to_string()),
                ..Default::default()
            }),
            devtools: Some(true),
        },
        ..Default::default()
    };
    let file = TelarSection {
        backend: Some(RendererBackend::Hardware),
        dev: DevSection {
            window: Some(WindowSection {
                width: Some(1024),
                position: Some("10,20".to_string()),
                ..Default::default()
            }),
            devtools: None,
        },
        ..Default::default()
    };

    let merged = merge_config(manifest, file);
    assert!(
        matches!(merged.backend, Some(RendererBackend::Hardware)),
        "telar.toml wins over the manifest"
    );
    assert_eq!(merged.dev.devtools, Some(true)); // omitted in telar.toml → manifest value survives
    let window = merged.dev.window.unwrap();
    assert_eq!(window.width, Some(1024)); // telar.toml wins
    assert_eq!(window.height, Some(600)); // falls back to manifest
    assert_eq!(window.position.as_deref(), Some("10,20")); // only in telar.toml
    assert_eq!(window.fullscreen.as_deref(), Some("disabled")); // only in manifest
}

#[test]
fn manifest_used_when_telar_toml_absent() {
    let manifest = TelarSection {
        backend: Some(RendererBackend::Software),
        ..Default::default()
    };
    let merged = merge_config(manifest, TelarSection::default());
    assert!(
        matches!(merged.backend, Some(RendererBackend::Software)),
        "with no telar.toml the manifest is the whole answer"
    );
    assert_eq!(merged.dev, DevSection::default());
}

/// A manifest as `resolve_package` would have read it, without a directory to read it from.
fn resolved(manifest: &str) -> ResolvedPackage {
    let parsed: CargoManifest = toml::from_str(manifest).unwrap();
    ResolvedPackage {
        workspace_root: PathBuf::new(),
        features: parsed.features.clone(),
        package: parsed.package,
        workspace_package: None,
        produces_cdylib: false,
    }
}

fn selected(manifest: &str, wanted: &str) -> Vec<String> {
    let mut args = Vec::new();
    resolved(manifest)
        .frontend_feature(wanted)
        .push_to(&mut args);
    args
}

const MULTI_TARGET: &str = r#"
[package]
name = "sandbox"
[features]
default = ["desktop"]
desktop = ["telar/desktop"]
tui = ["telar/tui"]
"#;

/// The bug: `--target tui` named `telar/tui` on top of a `default` that still stood for a window, so the terminal build carried a desktop one beside it — under Termux, a `platform-desktop` that cannot be compiled there at all.
#[test]
fn asking_for_another_target_turns_the_default_one_off() {
    assert_eq!(
        selected(MULTI_TARGET, "tui"),
        vec!["--no-default-features", "--features", "sandbox/tui"],
    );
}

#[test]
fn the_target_a_package_already_defaults_to_is_left_alone() {
    assert!(
        selected(MULTI_TARGET, "desktop").is_empty(),
        "naming what `default` already says would only drop the rest of it"
    );
}

/// What lets a project reach a frontend it never declared a feature for, which is every project before it has said anything.
#[test]
fn a_package_that_declares_no_such_feature_goes_through_telar() {
    let manifest = "[package]\nname = \"solo\"\n";
    assert_eq!(selected(manifest, "tui"), vec!["--features", "telar/tui"]);
}

/// `--no-default-features` is the only lever cargo has and it takes the whole list, so everything in `default` that is not another frontend has to be named again — or `--target tui` would silently turn off half of what the project builds with.
#[test]
fn the_rest_of_the_defaults_come_back() {
    let manifest = r#"
[package]
name = "app"
[features]
default = ["desktop", "telemetry", "telar/svg"]
desktop = ["telar/desktop"]
tui = ["telar/tui"]
telemetry = []
"#;
    assert_eq!(
        selected(manifest, "tui"),
        vec![
            "--no-default-features",
            "--features",
            "app/tui,app/telemetry,telar/svg",
        ],
    );
}

/// A `default` that names one feature of its own which names the frontend is still a project that chose that frontend — both when it is the one asked for, and when it is the one that has to be left behind.
#[test]
fn a_frontend_reached_through_another_feature_still_counts() {
    let manifest = r#"
[package]
name = "app"
[features]
default = ["everything"]
everything = ["desktop", "telemetry"]
desktop = ["telar/desktop"]
tui = ["telar/tui"]
telemetry = []
"#;
    assert_eq!(
        selected(manifest, "tui"),
        vec!["--no-default-features", "--features", "app/tui"],
        "`everything` carries the window, so re-naming it would bring it back"
    );
    assert!(selected(manifest, "desktop").is_empty());
}

/// `cargo telar dev` with no flags in a terminal project used to go looking for a window to hot-reload behind, because only the flag ever said "terminal" and the manifest had already said it.
#[test]
fn a_terminal_project_is_one_without_being_told() {
    let tui = r#"
[package]
name = "app"
[features]
default = ["tui"]
desktop = ["telar/desktop"]
tui = ["telar/tui"]
"#;
    assert!(resolved(tui).defaults_to_terminal());
    assert!(
        !resolved(MULTI_TARGET).defaults_to_terminal(),
        "a default that opens a window is not a terminal project, whatever else it can compile"
    );
    assert!(
        !resolved("[package]\nname = \"solo\"\n").defaults_to_terminal(),
        "a package that named nothing keeps telar's own default, which is a window"
    );
}

fn declared_features(manifest: &str) -> BTreeMap<String, toml::Value> {
    toml::from_str::<CargoManifest>(manifest).unwrap().features
}

#[test]
fn a_tooling_feature_no_member_names_is_reported_with_the_entry_that_locks_it() {
    let note = unlocked_tooling_note(
        &[declared_features("[package]\nname = \"app\"\n")],
        &["telar/dev"],
    )
    .expect("nothing in the workspace names `telar/dev`");
    assert!(note.contains("`telar/dev`"), "{note}");
    assert!(note.contains(&tooling_feature_entry()), "{note}");
}

#[test]
fn any_member_naming_it_in_either_spelling_locks_it() {
    let app = declared_features("[package]\nname = \"app\"\n");
    for spelling in ["telar/dev", "telar?/dev"] {
        let tools = declared_features(&format!(
            "[package]\nname = \"tools\"\n[features]\nlocked = [\"{spelling}\"]\n"
        ));
        assert_eq!(
            unlocked_tooling_note(&[app.clone(), tools], &["telar/dev"]),
            None,
            "`{spelling}`"
        );
    }
}

#[test]
fn only_the_features_left_unlocked_are_named() {
    let app = declared_features("[package]\nname = \"app\"\n[features]\ndev = [\"telar/dev\"]\n");
    let note = unlocked_tooling_note(&[app], &["telar/dev", "telar/hot-reload"])
        .expect("`telar/hot-reload` is named nowhere");
    assert!(note.contains("names `telar/hot-reload`, so"), "{note}");
}

#[test]
fn the_entry_cargo_telar_new_writes_locks_every_tooling_feature() {
    let manifest = format!(
        "[package]\nname = \"app\"\n[features]\n{}\n",
        tooling_feature_entry()
    );
    assert_eq!(
        unlocked_tooling_note(&[declared_features(&manifest)], &TOOLING_FEATURES),
        None
    );
}

#[test]
fn targets_web_is_true_for_either_spelling_of_the_browser_frontend() {
    assert!(resolved(MULTI_TARGET.replace("tui", "web").as_str()).targets_web());
    let dom = r#"
[package]
name = "app"
[features]
default = ["desktop"]
desktop = ["telar/desktop"]
web-dom = ["telar/web-dom"]
"#;
    assert!(resolved(dom).targets_web());
    assert!(
        !resolved(MULTI_TARGET).targets_web(),
        "a desktop/tui project names neither `web` nor `web-dom`"
    );
}

fn write_workspace_manifest(dir: &Path, contents: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("Cargo.toml"), contents).unwrap();
}

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("cargo_telar_config_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn has_web_profile_reads_the_workspace_root_manifest() {
    let dir = temp_dir("has_profile");
    write_workspace_manifest(
        &dir,
        "[package]\nname = \"app\"\n\n[profile.web]\ninherits = \"release\"\n",
    );
    assert!(has_web_profile(&dir));
}

#[test]
fn has_web_profile_is_false_when_the_manifest_names_no_such_profile() {
    let dir = temp_dir("no_profile");
    write_workspace_manifest(&dir, "[package]\nname = \"app\"\n\n[profile.release]\n");
    assert!(!has_web_profile(&dir));
}

#[test]
fn has_web_profile_is_false_when_there_is_no_manifest_at_all() {
    assert!(!has_web_profile(Path::new(
        "/nonexistent/cargo_telar_test_path"
    )));
}

fn table(manifest: &str) -> toml::Table {
    manifest.parse().unwrap()
}

/// Cargo's error for a feature `telar` no longer has names no fix, so the check names it: the crate each one became, the feature groups it stood for, and the prelude line that brings the tags back.
#[test]
fn the_catalogue_features_become_telar_components_with_their_groups() {
    let manifest = table(
        "[package]\nname = \"app\"\n[dependencies]\ntelar = { version = \"0.2\", features = [\"desktop\", \"components-overlays\", \"components-advanced\", \"components-base\"] }\n",
    );
    let moved = moved_telar_features(&manifest, None);
    assert_eq!(
        moved,
        BTreeSet::from([
            "components-advanced",
            "components-base",
            "components-overlays"
        ])
    );

    let hint = moved_features_hint("app", &moved, &[]).unwrap();
    assert!(
        hint.contains("cargo add -p app telar-components --features advanced,overlays\n"),
        "{hint}"
    );
    assert!(!hint.contains("telar-navigate"), "{hint}");
    assert!(hint.contains("prelude = [\"telar_components\"]"), "{hint}");
}

#[test]
fn the_whole_catalogue_is_every_group_and_navigate_is_its_own_crate() {
    let moved = moved_telar_features(
        &table(
            "[package]\nname = \"app\"\n[dependencies]\ntelar = { version = \"0.2\", features = [\"components\", \"navigate\"] }\n",
        ),
        None,
    );
    let hint = moved_features_hint("app", &moved, &[]).unwrap();
    assert!(
        hint.contains("cargo add -p app telar-components --features advanced,chrome,overlays"),
        "{hint}"
    );
    assert!(hint.contains("cargo add -p app telar-navigate"), "{hint}");
    assert!(
        hint.contains("prelude = [\"telar_components\", \"telar_navigate\"]"),
        "{hint}"
    );
}

/// `components-base` was the catalogue with no group, which is what `telar-components` is with no features.
#[test]
fn the_base_catalogue_needs_no_feature() {
    let moved = moved_telar_features(
        &table(
            "[package]\nname = \"app\"\n[dependencies]\ntelar = { version = \"0.2\", features = [\"components-base\"] }\n",
        ),
        None,
    );
    let hint = moved_features_hint("app", &moved, &[]).unwrap();
    assert!(
        hint.contains("cargo add -p app telar-components\n"),
        "{hint}"
    );
    assert!(!hint.contains("--features"), "{hint}");
}

/// Everywhere a manifest can turn a `telar` feature on: a `[features]` entry in either spelling, a target-specific dependency, a renamed one, and the workspace entry a `workspace = true` inherits.
#[test]
fn a_moved_feature_is_found_wherever_the_manifest_turns_it_on() {
    for spelling in ["telar/navigate", "telar?/navigate"] {
        let manifest = table(&format!(
            "[package]\nname = \"app\"\n[dependencies]\ntelar = \"0.2\"\n[features]\nroutes = [\"{spelling}\"]\n"
        ));
        assert_eq!(
            moved_telar_features(&manifest, None),
            BTreeSet::from(["navigate"]),
            "{spelling}"
        );
    }
    let target = table(
        "[package]\nname = \"app\"\n[target.'cfg(unix)'.dependencies]\ntelar = { version = \"0.2\", features = [\"components-chrome\"] }\n",
    );
    assert_eq!(
        moved_telar_features(&target, None),
        BTreeSet::from(["components-chrome"])
    );
    let renamed = table(
        "[package]\nname = \"app\"\n[dependencies]\nui = { package = \"telar\", version = \"0.2\" }\n[features]\nnav = [\"ui/navigate\"]\n",
    );
    assert_eq!(
        moved_telar_features(&renamed, None),
        BTreeSet::from(["navigate"])
    );
    let workspace = table(
        "[workspace.dependencies]\ntelar = { version = \"0.2\", features = [\"components\"] }\n",
    );
    let inheriting =
        table("[package]\nname = \"app\"\n[dependencies]\ntelar = { workspace = true }\n");
    assert_eq!(
        moved_telar_features(
            &inheriting,
            workspace
                .get("workspace")
                .and_then(|w| w.get("dependencies"))
                .and_then(toml::Value::as_table)
        ),
        BTreeSet::from(["components"])
    );
}

#[test]
fn a_project_that_names_none_of_them_gets_no_hint() {
    let manifest = table(
        "[package]\nname = \"app\"\n[dependencies]\ntelar = { version = \"0.2\", features = [\"desktop\"] }\ntelar-components = \"0.2\"\n[features]\nnavigate = []\n",
    );
    let moved = moved_telar_features(&manifest, None);
    assert!(moved.is_empty(), "{moved:?}");
    assert_eq!(moved_features_hint("app", &moved, &[]), None);
}

/// A prelude that already names the crate is not told to name it again; one that names others keeps them in the line it is told to write.
#[test]
fn the_prelude_line_keeps_what_is_declared_and_is_left_out_when_complete() {
    let moved = BTreeSet::from(["components", "navigate"]);
    let declared = [telar_project::PreludeEntry::parse("my-plugin").unwrap()];
    let hint = moved_features_hint("app", &moved, &declared).unwrap();
    assert!(
        hint.contains("prelude = [\"my_plugin\", \"telar_components\", \"telar_navigate\"]"),
        "{hint}"
    );

    let complete = [
        telar_project::PreludeEntry::parse("telar-components").unwrap(),
        telar_project::PreludeEntry::parse("telar-navigate").unwrap(),
    ];
    let hint = moved_features_hint("app", &moved, &complete).unwrap();
    assert!(!hint.contains("prelude"), "{hint}");
}
