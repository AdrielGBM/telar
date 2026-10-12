use super::*;

/// `cargo metadata` reports the target directory after it has already applied `CARGO_TARGET_DIR`, so a
/// packager reading `target_directory` picks up the override without knowing that variable exists.
#[test]
fn find_target_dir_honours_cargo_target_dir() {
    let root = find_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("this crate is a workspace member");
    let custom = std::env::temp_dir().join(format!(
        "telar-target-dir-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    let mut cargo = std::process::Command::new("cargo");
    cargo.env("CARGO_TARGET_DIR", &custom);
    let resolved = target_dir_reported_by(cargo, &root);

    assert_eq!(
        resolved, custom,
        "expected cargo metadata's target_directory to reflect CARGO_TARGET_DIR"
    );
}

/// When cargo cannot be run at all, the plain workspace-relative join is the only answer left — the same
/// fallback [`resolve_telar_version`] uses.
#[test]
fn find_target_dir_falls_back_when_cargo_is_unavailable() {
    let root = Path::new("/nonexistent/workspace/for/telar-target-dir-test");
    let resolved = target_dir_reported_by(
        std::process::Command::new("/nonexistent/cargo-for-telar-target-dir-test"),
        root,
    );

    assert_eq!(resolved, root.join("target"));
}

/// Every crate here is published together and depends on the others by an exact version, so the version literals in `[workspace.dependencies]` have to move with `[workspace.package] version`.
///
/// Nothing enforces that today, and the two are written in different places: a member's own version is inherited (`version = { workspace = true }`), but the version a *dependent* asks for is a literal string beside the path. A release that bumps the package version and leaves those literals behind still builds here — `path` wins over `version` inside a workspace, so every in-tree build resolves the sibling on disk and passes. It is the published artifact that breaks: `telar 0.2.0` would carry a dependency on `telar-dynamic ^0.1.8`, cargo would resolve the *old published* copy, and an application depending on both gets two copies of `renderer-assets` — which is the mismatch `telar-dynamic`'s own crate docs warn about, arrived at without anyone writing a wrong version anywhere.
///
/// So: one version for the whole workspace, checked from the manifest rather than trusted.
#[test]
fn every_internal_dependency_asks_for_the_version_this_workspace_publishes() {
    let root = find_workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")))
        .expect("these crates are workspace members and the root holds the version they share");
    let manifest: toml::Table = std::fs::read_to_string(root.join("Cargo.toml"))
        .expect("the workspace root has a manifest")
        .parse()
        .expect("the workspace manifest is valid TOML");

    let workspace = manifest["workspace"]
        .as_table()
        .expect("[workspace] is a table");
    let published = workspace["package"]["version"]
        .as_str()
        .expect("[workspace.package] version is a string");

    let mut wrong = Vec::new();
    let mut unversioned = Vec::new();
    for (name, spec) in workspace["dependencies"]
        .as_table()
        .expect("[workspace.dependencies] is a table")
    {
        // A `path` is what makes it one of ours. Everything else is a third-party version this says nothing about.
        let Some(spec) = spec.as_table().filter(|spec| spec.contains_key("path")) else {
            continue;
        };
        match spec.get("version").and_then(toml::Value::as_str) {
            Some(version) if version == published => {}
            // Not a nitpick: a path dependency with no version is stripped of its path on publish and left with nothing, which cargo refuses — so this crate could not be released at all.
            None => unversioned.push(name.clone()),
            Some(version) => wrong.push(format!("{name} asks for {version}")),
        }
    }

    assert!(
        wrong.is_empty(),
        "[workspace.package] publishes {published}, but these ask for something else — bump them together \
         or the published crates resolve each other's older copies:\n  {}",
        wrong.join("\n  ")
    );
    assert!(
        unversioned.is_empty(),
        "a path dependency with no version cannot be published, because publishing strips the path:\n  {}",
        unversioned.join("\n  ")
    );
}

#[test]
fn the_telar_version_is_the_resolved_package_named_telar() {
    let metadata = serde_json::json!({
        "packages": [
            { "name": "telar-macros", "version": "9.0.0" },
            { "name": "telar", "version": "1.2.3" },
        ]
    });
    assert_eq!(telar_version_in(&metadata).as_deref(), Some("1.2.3"));
    assert_eq!(
        telar_version_in(&serde_json::json!({ "packages": [] })),
        None
    );
}

fn members_scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("telar_members_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn member_dirs_expand_globs_and_keep_only_real_crates() {
    let root = members_scratch("globs");
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*\"]\n",
    )
    .unwrap();
    for member in ["crates/a", "crates/b"] {
        std::fs::create_dir_all(root.join(member)).unwrap();
        std::fs::write(
            root.join(member).join("Cargo.toml"),
            "[package]\nname = \"x\"\n",
        )
        .unwrap();
    }
    std::fs::create_dir_all(root.join("crates/not_a_crate")).unwrap();

    let mut dirs = workspace_member_dirs(&root);
    dirs.sort();
    assert_eq!(dirs, vec![root.join("crates/a"), root.join("crates/b")]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_workspace_manifest_that_is_also_a_package_is_a_member_too() {
    let root = members_scratch("root_pkg");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"root\"\n\n[workspace]\nmembers = []\n",
    )
    .unwrap();

    assert_eq!(workspace_member_dirs(&root), vec![root.clone()]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_lone_package_with_no_workspace_table_is_its_own_member() {
    let root = members_scratch("lone_pkg");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"solo\"\n").unwrap();

    assert_eq!(workspace_member_dirs(&root), vec![root.clone()]);
    let _ = std::fs::remove_dir_all(&root);
}
