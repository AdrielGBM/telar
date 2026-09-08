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
