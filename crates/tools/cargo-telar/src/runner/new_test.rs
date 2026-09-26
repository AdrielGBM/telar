use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cargo_telar_new_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn generated_manifest(target: Target) -> toml::Table {
    toml::from_str(&manifest("my-app", target, None)).expect("generated manifest is not valid TOML")
}

#[test]
fn the_gitignore_covers_the_generated_telar_directory_at_any_depth() {
    let files = scaffold_files("my-app", Target::Web, None);
    let (_, gitignore) = files
        .iter()
        .find(|(path, _)| *path == ".gitignore")
        .expect("a .gitignore is scaffolded");
    assert!(
        gitignore.lines().any(|line| line == ".telar/"),
        "`app!` also writes `src/.telar/`, which a root-anchored `/.telar` leaves tracked: {gitignore:?}"
    );
}

#[test]
fn every_target_writes_itself_as_the_default_feature() {
    for target in [Target::Desktop, Target::Tui, Target::Web, Target::Android] {
        let manifest = generated_manifest(target);
        let features = manifest["features"].as_table().expect("features table");
        let default = features["default"].as_array().expect("default array");
        assert_eq!(
            default[0].as_str(),
            Some(target_name(target)),
            "default does not name the chosen target"
        );
        // Every target stays one word away, whichever one the project started on.
        for name in ["desktop", "tui", "web", "android"] {
            assert!(features.contains_key(name), "`{name}` is missing");
        }
    }
}

#[test]
fn the_telar_dependency_names_no_target_of_its_own() {
    let manifest = generated_manifest(Target::Tui);
    let telar = &manifest["dependencies"]["telar"];
    assert_eq!(telar["default-features"].as_bool(), Some(false));
    let features: Vec<_> = telar["features"]
        .as_array()
        .expect("features array")
        .iter()
        .filter_map(|f| f.as_str())
        .collect();
    for target in ["desktop", "tui", "web", "android"] {
        assert!(
            !features.contains(&target),
            "`{target}` belongs under [features], not on the dependency"
        );
    }
}

#[test]
fn the_generated_config_parses() {
    for target in [Target::Desktop, Target::Tui, Target::Web, Target::Android] {
        toml::from_str::<toml::Table>(&config("my-app", target))
            .expect("generated telar.toml is not valid TOML");
    }
}

#[test]
fn a_name_that_would_not_compile_is_refused() {
    assert!(
        validate_name("my-app").is_ok(),
        "a hyphen is fine in a crate name"
    );
    assert!(
        validate_name("my_app2").is_ok(),
        "and so are underscores and digits"
    );
    assert!(
        validate_name("").is_err(),
        "an empty name is not a crate name"
    );
    assert!(
        validate_name("2fast").is_err(),
        "a leading digit is not a Rust identifier"
    );
    assert!(validate_name("my app").is_err(), "a space is not allowed");
    assert!(validate_name("my.app").is_err(), "nor is a dot");
}

#[test]
fn every_tooling_feature_is_named_where_the_lockfile_sees_it() {
    for target in [Target::Desktop, Target::Tui, Target::Web, Target::Android] {
        let manifest = generated_manifest(target);
        let features = manifest["features"].as_table().expect("features table");
        let tooling: Vec<_> = features["tooling"]
            .as_array()
            .expect("tooling array")
            .iter()
            .filter_map(|f| f.as_str())
            .collect();
        assert_eq!(tooling, crate::runner::config::TOOLING_FEATURES);
        assert!(
            !features["default"]
                .as_array()
                .expect("default array")
                .iter()
                .any(|f| f.as_str() == Some("tooling")),
            "`tooling` exists for the lockfile, never for a build"
        );
    }
}

#[test]
fn the_dom_renderer_writes_web_dom_as_the_default() {
    let manifest =
        toml::from_str::<toml::Table>(&manifest("my-app", Target::Web, Some(WebRenderer::Dom)))
            .expect("generated manifest is not valid TOML");
    let features = manifest["features"].as_table().expect("features table");
    let default = features["default"].as_array().expect("default array");
    assert_eq!(default[0].as_str(), Some("web-dom"));
    assert!(
        features.contains_key("web-dom"),
        "`web-dom` needs its own entry, whichever renderer is the default"
    );
}

#[test]
fn a_web_target_without_a_renderer_still_defaults_to_web() {
    let manifest = generated_manifest(Target::Web);
    let features = manifest["features"].as_table().expect("features table");
    let default = features["default"].as_array().expect("default array");
    assert_eq!(default[0].as_str(), Some("web"));
}

#[test]
fn only_the_web_target_gets_a_profile_web() {
    for target in [Target::Desktop, Target::Tui, Target::Web, Target::Android] {
        let manifest = generated_manifest(target);
        let has_profile_web = manifest
            .get("profile")
            .and_then(|p| p.as_table())
            .is_some_and(|p| p.contains_key("web"));
        assert_eq!(
            has_profile_web,
            target == Target::Web,
            "[profile.web] should exist for a web target only"
        );
    }
}

#[test]
fn profile_web_matches_the_one_release_web_builds_are_checked_against() {
    let manifest = generated_manifest(Target::Web);
    let profile = manifest["profile"]["web"]
        .as_table()
        .expect("[profile.web] table");
    assert_eq!(profile["inherits"].as_str(), Some("release"));
    assert_eq!(profile["opt-level"].as_str(), Some("z"));
    assert_eq!(profile["panic"].as_str(), Some("abort"));
}

#[test]
fn init_scaffolds_into_a_directory_that_already_has_unrelated_files() {
    let dir = temp_dir("init_nonempty");
    std::fs::write(dir.join("README.md"), "hello\n").unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();

    let files = scaffold_files("my-app", Target::Desktop, None);
    assert!(
        conflicts_in(&dir, &files).is_empty(),
        "an unrelated README and .git should never conflict with the scaffold"
    );
    write_all(&dir, &files);
    assert!(dir.join("Cargo.toml").exists());
    assert!(dir.join("README.md").exists(), "unrelated file is kept");
}

#[test]
fn init_refuses_to_overwrite_a_file_it_would_itself_write() {
    let dir = temp_dir("init_conflict");
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"other\"\n").unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "// mine\n").unwrap();

    let files = scaffold_files("my-app", Target::Desktop, None);
    let conflicts = conflicts_in(&dir, &files);
    assert!(conflicts.contains(&"Cargo.toml"));
    assert!(conflicts.contains(&"src/lib.rs"));
    assert!(
        !conflicts.contains(&"telar.toml"),
        "a file that is not there yet is not a conflict"
    );

    let original = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    assert_eq!(
        original, "[package]\nname = \"other\"\n",
        "an existing Cargo.toml is refused, never merged or overwritten"
    );
}

#[test]
fn init_derives_the_package_name_from_the_current_directory_when_the_path_is_a_dot() {
    let name = name_from_path(Path::new("."), || Some(PathBuf::from("/wherever/my-app")));
    assert_eq!(name.as_deref(), Some("my-app"));
}

#[test]
fn a_real_path_names_itself_without_asking_for_the_current_directory() {
    let name = name_from_path(Path::new("projects/my-app"), || {
        panic!("a non-dot path should never need the current directory")
    });
    assert_eq!(name.as_deref(), Some("my-app"));
}
