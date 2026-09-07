use super::*;

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
