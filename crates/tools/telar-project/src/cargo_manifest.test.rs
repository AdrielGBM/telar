use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "telar_cargo_manifest_{name}_{}",
        std::process::id()
    ));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn parsed(text: &str) -> CargoManifest {
    CargoManifest::parse(Path::new("/nonexistent/package"), text).unwrap()
}

#[test]
fn the_library_is_named_by_its_lib_name_rather_than_its_package() {
    let manifest = parsed("[package]\nname = \"my-app\"\n\n[lib]\nname = \"other\"\n");
    assert_eq!(manifest.lib_name().as_deref(), Some("other"));
}

#[test]
fn a_library_without_a_name_takes_the_package_name_spelled_as_a_crate() {
    let manifest = parsed("[package]\nname = \"my-app\"\n");
    assert_eq!(manifest.lib_name().as_deref(), Some("my_app"));
}

#[test]
fn declared_binaries_are_named_by_their_bin_name() {
    let manifest = parsed(
        "[package]\nname = \"my-app\"\n\n[[bin]]\nname = \"launcher\"\npath = \"src/launch.rs\"\n",
    );
    assert_eq!(manifest.bin_names(), ["launcher"]);
}

#[test]
fn binaries_cargo_discovers_are_named_after_the_package_and_their_files() {
    let dir = scratch("discovered");
    std::fs::create_dir_all(dir.join("src/bin/tool")).unwrap();
    std::fs::write(dir.join("src/main.rs"), "fn main() {}").unwrap();
    std::fs::write(dir.join("src/bin/helper.rs"), "fn main() {}").unwrap();
    std::fs::write(dir.join("src/bin/tool/main.rs"), "fn main() {}").unwrap();
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname = \"my-app\"\n").unwrap();
    let names = CargoManifest::read(&dir).unwrap().bin_names();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(names, ["my-app", "helper", "tool"]);
}

#[test]
fn a_bin_declared_at_main_rs_replaces_the_one_cargo_would_discover_there() {
    let dir = scratch("renamed_main");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.rs"), "fn main() {}").unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"my-app\"\n\n[[bin]]\nname = \"launcher\"\npath = \"src/main.rs\"\n",
    )
    .unwrap();
    let names = CargoManifest::read(&dir).unwrap().bin_names();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(names, ["launcher"]);
}

#[test]
fn autobins_false_leaves_only_the_declared_binaries() {
    let dir = scratch("no_autobins");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/main.rs"), "fn main() {}").unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"my-app\"\nautobins = false\n",
    )
    .unwrap();
    let names = CargoManifest::read(&dir).unwrap().bin_names();
    std::fs::remove_dir_all(&dir).ok();
    assert!(names.is_empty(), "{names:?}");
}

#[test]
fn a_missing_manifest_loads_as_none_and_one_that_is_not_toml_as_an_error() {
    let dir = scratch("load");
    assert!(matches!(CargoManifest::load(&dir), Ok(None)));
    std::fs::write(dir.join("Cargo.toml"), "[package\n").unwrap();
    let loaded = CargoManifest::load(&dir);
    std::fs::remove_dir_all(&dir).ok();
    assert!(matches!(loaded, Err(ManifestError::Invalid { .. })));
}

#[test]
fn dependency_tables_include_the_target_specific_copies() {
    let manifest = parsed(
        "[dependencies]\na = \"1\"\n\n[target.'cfg(unix)'.dependencies]\nb = \"1\"\n\n[dev-dependencies]\nc = \"1\"\n",
    );
    let names: Vec<&String> = manifest
        .dependency_tables("dependencies")
        .flat_map(|table| table.keys())
        .collect();
    assert_eq!(names, ["a", "b"]);
}

#[test]
fn declared_target_paths_name_each_kind() {
    let manifest = parsed(
        "[lib]\npath = \"lib/root.rs\"\n\n[[bin]]\nname = \"b\"\npath = \"tools/b.rs\"\n\n[[test]]\nname = \"t\"\npath = \"src/t.test.rs\"\n",
    );
    assert_eq!(
        manifest.declared_target_paths(),
        [
            (TargetKind::Lib, "lib/root.rs"),
            (TargetKind::Bin, "tools/b.rs"),
            (TargetKind::Test, "src/t.test.rs"),
        ]
    );
}
