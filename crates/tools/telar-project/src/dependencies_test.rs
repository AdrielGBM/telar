use super::*;

fn declares_with_implicit(manifest: &str, dependency: &str) -> bool {
    declares(&manifest.parse().unwrap(), true, dependency)
}

fn declares_explicit_only(manifest: &str, dependency: &str) -> bool {
    declares(&manifest.parse().unwrap(), false, dependency)
}

const IMPLICIT: &str = r#"
    [dependencies]
    telar-devtools = { workspace = true, optional = true }
"#;

/// What `cargo add --optional` writes.
const EXPLICIT: &str = r#"
    [dependencies]
    telar-devtools = { version = "0.2", optional = true }

    [features]
    telar-devtools = ["dep:telar-devtools"]
"#;

#[test]
fn an_optional_dependency_with_its_implicit_feature_is_declared_before_2024() {
    assert!(declares_with_implicit(IMPLICIT, "telar-devtools"));
}

/// The 2024 edition drops implicit features, so `cfg(feature = "telar-devtools")` would name one cargo never defines.
#[test]
fn an_implicit_feature_is_not_declared_from_2024() {
    assert!(!declares_explicit_only(IMPLICIT, "telar-devtools"));
}

#[test]
fn an_explicit_feature_that_activates_it_is_declared_in_every_edition() {
    assert!(declares_with_implicit(EXPLICIT, "telar-devtools"));
    assert!(declares_explicit_only(EXPLICIT, "telar-devtools"));
}

#[test]
fn a_feature_reaching_it_through_another_feature_is_declared() {
    let manifest = r#"
        [dependencies]
        telar-devtools = { version = "0.2", optional = true }

        [features]
        telar-devtools = ["inspect"]
        inspect = ["dep:telar-devtools"]
    "#;
    assert!(declares_explicit_only(manifest, "telar-devtools"));
}

#[test]
fn a_target_specific_optional_dependency_is_declared() {
    let manifest = r#"
        [target.'cfg(not(target_arch = "wasm32"))'.dependencies]
        telar-devtools = { version = "0.2", optional = true }

        [features]
        telar-devtools = ["dep:telar-devtools"]
    "#;
    assert!(declares_explicit_only(manifest, "telar-devtools"));
}

/// A required dependency is compiled into every build, shipping ones included, and has no feature to gate on.
#[test]
fn a_required_dependency_is_not_declared() {
    let manifest = r#"
        [dependencies]
        telar-devtools = "0.2"
    "#;
    assert!(!declares_with_implicit(manifest, "telar-devtools"));
}

#[test]
fn an_absent_dependency_is_not_declared() {
    let manifest = r#"
        [dependencies]
        telar = "0.2"

        [features]
        telar-devtools = []
    "#;
    assert!(!declares_with_implicit(manifest, "telar-devtools"));
}

#[test]
fn a_dev_dependency_is_not_declared() {
    let manifest = r#"
        [dev-dependencies]
        telar-devtools = { version = "0.2", optional = true }
    "#;
    assert!(!declares_with_implicit(manifest, "telar-devtools"));
}

/// `dep:` under another feature removes the implicit one.
#[test]
fn an_activation_under_another_name_hides_the_implicit_feature() {
    let manifest = r#"
        [dependencies]
        telar-devtools = { version = "0.2", optional = true }

        [features]
        inspect = ["dep:telar-devtools"]
    "#;
    assert!(!declares_with_implicit(manifest, "telar-devtools"));
}

#[test]
fn a_feature_of_that_name_that_never_activates_it_is_not_declared() {
    let manifest = r#"
        [dependencies]
        telar-devtools = { version = "0.2", optional = true }

        [features]
        telar-devtools = []
        inspect = ["dep:telar-devtools"]
    "#;
    assert!(!declares_with_implicit(manifest, "telar-devtools"));
}

#[test]
fn an_unreadable_manifest_declares_nothing() {
    let missing = std::env::temp_dir().join("telar-project-no-such-package");
    assert!(!declares_optional_dependency(&missing, "telar-devtools"));
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "telar_optional_dependency_{name}_{}",
        std::process::id()
    ));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_package_without_an_edition_gets_the_implicit_feature() {
    let dir = scratch("no_edition");
    std::fs::write(
        dir.join("Cargo.toml"),
        format!("[package]\nname = \"app\"\n{IMPLICIT}"),
    )
    .unwrap();
    let declared = declares_optional_dependency(&dir, "telar-devtools");
    std::fs::remove_dir_all(&dir).ok();
    assert!(declared);
}

#[test]
fn an_edition_inherited_from_the_workspace_is_followed() {
    let root = scratch("inherited");
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"app\"]\n\n[workspace.package]\nedition = \"2024\"\n",
    )
    .unwrap();
    let app = root.join("app");
    std::fs::create_dir_all(&app).unwrap();
    let package = "[package]\nname = \"app\"\nedition = { workspace = true }\n";
    std::fs::write(app.join("Cargo.toml"), format!("{package}{IMPLICIT}")).unwrap();
    let implicit = declares_optional_dependency(&app, "telar-devtools");
    std::fs::write(app.join("Cargo.toml"), format!("{package}{EXPLICIT}")).unwrap();
    let explicit = declares_optional_dependency(&app, "telar-devtools");
    std::fs::remove_dir_all(&root).ok();
    assert!(!implicit);
    assert!(explicit);
}
