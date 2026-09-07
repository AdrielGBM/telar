use super::*;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cargo_telar_bake_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn member_dirs_expands_globs_and_keeps_only_real_crates() {
    let root = temp_dir("members");
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

    let mut dirs = member_dirs(&root);
    dirs.sort();
    assert_eq!(dirs, vec![root.join("crates/a"), root.join("crates/b")]);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_workspace_manifest_that_is_also_a_package_bakes_itself_too() {
    let root = temp_dir("root_pkg");
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"root\"\n\n[workspace]\nmembers = []\n",
    )
    .unwrap();

    assert_eq!(member_dirs(&root), vec![root.clone()]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_lone_package_with_no_workspace_table_bakes_itself() {
    let root = temp_dir("lone_pkg");
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"solo\"\n").unwrap();

    assert_eq!(member_dirs(&root), vec![root.clone()]);
    let _ = std::fs::remove_dir_all(&root);
}
