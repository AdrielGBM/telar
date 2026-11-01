use icons_core::{License, OnUnlisted, SetInfo};
use serde_json::json;

use super::*;
use crate::icons::RecordedIcon;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "telar_icon_dependencies_{name}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn crate_dir(root: &Path, name: &str, library: bool) -> PathBuf {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    if library {
        std::fs::write(dir.join("telar.toml"), "[telar]\nlibrary = true\n").unwrap();
    }
    dir
}

fn package(id: &str, name: &str, dir: &Path, registry: bool) -> serde_json::Value {
    json!({
        "id": id,
        "name": name,
        "manifest_path": dir.join("Cargo.toml").display().to_string(),
        "source": if registry { json!("registry+https://github.com/rust-lang/crates.io-index") } else { json!(null) },
    })
}

fn dep(pkg: &str, kinds: &[Option<&str>]) -> serde_json::Value {
    json!({
        "pkg": pkg,
        "dep_kinds": kinds.iter().map(|kind| json!({ "kind": kind })).collect::<Vec<_>>(),
    })
}

/// An application built with a local library, a registry crate that pulls in a registry library, and a local crate; a library only its tests use and a registry crate that is no library carry nothing it ships.
fn graph(root: &Path) -> (DependencyGraph, PathBuf, PathBuf) {
    let app = crate_dir(root, "app", false);
    let ui = crate_dir(root, "ui", false);
    let local_lib = crate_dir(root, "local-lib", true);
    let middle = crate_dir(root, "middle", false);
    let deep_lib = crate_dir(root, "deep-lib", true);
    let test_lib = crate_dir(root, "test-lib", true);
    let metadata = json!({
        "packages": [
            package("app", "app", &app, false),
            package("ui", "ui", &ui, false),
            package("local-lib", "local-lib", &local_lib, false),
            package("middle", "middle", &middle, true),
            package("deep-lib", "deep-lib", &deep_lib, true),
            package("test-lib", "test-lib", &test_lib, false),
        ],
        "resolve": { "nodes": [
            { "id": "app", "deps": [
                dep("ui", &[None]),
                dep("middle", &[None, Some("build")]),
                dep("test-lib", &[Some("dev")]),
            ] },
            { "id": "ui", "deps": [dep("local-lib", &[None])] },
            { "id": "middle", "deps": [dep("deep-lib", &[None])] },
            { "id": "local-lib", "deps": [] },
            { "id": "deep-lib", "deps": [] },
            { "id": "test-lib", "deps": [] },
        ] },
    });
    (DependencyGraph::from_metadata(&metadata), app, ui)
}

#[test]
fn the_crates_that_can_carry_icons_are_found_at_any_depth_through_normal_dependencies() {
    let root = temp_dir("graph");
    let (graph, app, _) = graph(&root);
    let found: Vec<(String, bool)> = graph
        .icon_dependencies(&app)
        .unwrap()
        .into_iter()
        .map(|dependency| (dependency.name, dependency.library))
        .collect();
    assert_eq!(
        found,
        vec![
            ("deep-lib".to_string(), true),
            ("local-lib".to_string(), true),
            ("ui".to_string(), false),
        ]
    );
    assert!(graph.icon_dependencies(&root.join("elsewhere")).is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn members_bake_after_the_members_they_are_built_with() {
    let root = temp_dir("order");
    let (graph, app, ui) = graph(&root);
    let local_lib = root.join("local-lib");
    let stray = root.join("stray");
    let order = graph.bake_order(vec![
        app.clone(),
        stray.clone(),
        ui.clone(),
        local_lib.clone(),
    ]);
    assert_eq!(order, vec![local_lib, ui, app, stray]);
    let _ = std::fs::remove_dir_all(&root);
}

fn set(spdx: &str) -> RecordedSet {
    RecordedSet {
        own: false,
        info: Some(SetInfo {
            license: Some(License {
                spdx: Some(spdx.to_string()),
                ..License::default()
            }),
            ..SetInfo::default()
        }),
    }
}

/// A local crate whose own bake recorded `ids`, every set under `spdx`, beside icons of a dependency of its own that it must not pass on.
fn local_crate(root: &Path, name: &str, spdx: &str, ids: &[&str]) -> IconDependency {
    let dir = crate_dir(root, name, false);
    let record = IconRecord {
        format: 1,
        provider: None,
        sets: ids
            .iter()
            .map(|id| (id.split_once(':').unwrap().0.to_string(), set(spdx)))
            .collect(),
        icons: ids
            .iter()
            .map(|id| (id.to_string(), RecordedIcon::default()))
            .collect(),
        dependencies: BTreeMap::from([(
            "transitive".to_string(),
            CrateIcons {
                sets: BTreeMap::from([("other".to_string(), set("MIT"))]),
                icons: BTreeSet::from(["other:x".to_string()]),
            },
        )]),
    };
    std::fs::create_dir_all(dir.join(".telar")).unwrap();
    std::fs::write(
        dir.join(".telar/icons.json"),
        serde_json::to_string(&record).unwrap(),
    )
    .unwrap();
    IconDependency {
        name: name.to_string(),
        dir,
        library: false,
    }
}

#[test]
fn a_dependencys_icons_are_its_own_and_judged_by_the_packages_policy() {
    let root = temp_dir("judged");
    let dependencies = [
        local_crate(&root, "cards", "CC-BY-4.0", &["attrib:dot", "attrib:ring"]),
        local_crate(&root, "nav", "MIT", &["demo:home"]),
        IconDependency {
            name: "plain".to_string(),
            dir: crate_dir(&root, "plain", false),
            library: false,
        },
    ];
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    let icons = dependency_icons(
        None,
        Some(&dependencies),
        &LicensePolicy::default(),
        &mut warnings,
        &mut errors,
    );
    assert_eq!(icons.keys().collect::<Vec<_>>(), vec!["cards", "nav"]);
    assert_eq!(
        icons["cards"].icons,
        BTreeSet::from(["attrib:dot".to_string(), "attrib:ring".to_string()])
    );
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].starts_with("`cards`, which this package is built with"),
        "{}",
        warnings[0]
    );
    assert!(warnings[0].contains("CC-BY-4.0"), "{}", warnings[0]);

    let strict = LicensePolicy {
        allow: Vec::new(),
        unlisted: OnUnlisted::Fail,
    };
    let (mut warnings, mut errors) = (Vec::new(), Vec::new());
    let kept = dependency_icons(
        None,
        Some(&dependencies),
        &strict,
        &mut warnings,
        &mut errors,
    );
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        kept.contains_key("cards"),
        "a refused dependency's icons still ship, so they stay in the notice"
    );

    let accepting = LicensePolicy {
        allow: vec!["attrib".to_string()],
        unlisted: OnUnlisted::Fail,
    };
    let (mut warnings, mut errors) = (Vec::new(), Vec::new());
    dependency_icons(
        None,
        Some(&dependencies),
        &accepting,
        &mut warnings,
        &mut errors,
    );
    assert!(
        warnings.is_empty() && errors.is_empty(),
        "{warnings:?} {errors:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_bake_that_does_not_know_the_dependencies_keeps_and_rejudges_the_recorded_ones() {
    let previous = IconRecord {
        dependencies: BTreeMap::from([(
            "cards".to_string(),
            CrateIcons {
                sets: BTreeMap::from([("attrib".to_string(), set("CC-BY-4.0"))]),
                icons: BTreeSet::from(["attrib:dot".to_string()]),
            },
        )]),
        ..IconRecord::default()
    };
    let (mut warnings, mut errors) = (Vec::new(), Vec::new());
    let kept = dependency_icons(
        Some(&previous),
        None,
        &LicensePolicy::default(),
        &mut warnings,
        &mut errors,
    );
    assert_eq!(kept, previous.dependencies);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}
