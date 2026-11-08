use super::*;

fn write(root: &Path, relative: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "").unwrap();
}

fn collected(root: &Path, scope: &Path) -> Vec<String> {
    let mut found = Vec::new();
    collect(scope, &mut found);
    found.sort();
    found
        .iter()
        .map(|path| telar_project::relative_slashed(path, root).unwrap())
        .collect()
}

#[test]
fn only_the_sources_cargo_compiles_are_collected() {
    let root = std::env::temp_dir().join(format!("cargo_telar_fmt_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for file in [
        "Cargo.toml",
        "build.rs",
        "src/lib.rs",
        "src/page.rsx",
        "src/.telar/generated.rs",
        "tests/it.rs",
        "examples/demo.rs",
        "golden/page.rs",
        "scratch/notes.rs",
        "target/debug/out.rs",
        "custom/main.rs",
        "member/Cargo.toml",
        "member/src/lib.rs",
        "member/golden/page.rs",
    ] {
        write(&root, file);
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"p\"\n[[bin]]\nname = \"b\"\npath = \"custom/main.rs\"\n",
    )
    .unwrap();

    let everything = [
        "build.rs",
        "custom/main.rs",
        "examples/demo.rs",
        "member/src/lib.rs",
        "src/lib.rs",
        "src/page.rsx",
        "tests/it.rs",
    ];
    assert_eq!(collected(&root, &root), everything);
    assert_eq!(
        collected(&root, &root.join("member")),
        ["member/src/lib.rs"]
    );
    assert_eq!(
        collected(&root, &root.join("src")),
        ["src/lib.rs", "src/page.rsx"]
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn formatting_through_a_symlink_rewrites_the_file_it_names_and_keeps_its_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("page.rsx");
    std::fs::write(&source, "before").unwrap();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o640)).unwrap();
    let link = dir.path().join("link.rsx");
    std::os::unix::fs::symlink(&source, &link).unwrap();

    replace_contents(&link, "after").unwrap();

    assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
    assert_eq!(std::fs::read_link(&link).unwrap(), source);
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "after");
    assert_eq!(
        std::fs::metadata(&source).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}
