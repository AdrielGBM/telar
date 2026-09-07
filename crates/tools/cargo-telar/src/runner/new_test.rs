use super::*;

fn generated_manifest(target: Target) -> toml::Table {
    toml::from_str(&manifest("my-app", target)).expect("generated manifest is not valid TOML")
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
