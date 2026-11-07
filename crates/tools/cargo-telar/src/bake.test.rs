//! `cargo telar bake` run as a user runs it, on a package of its own outside this workspace: what its exit status says about a catalog that does not parse.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn package(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("cargo_telar_bake_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("locales")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n"
        ),
    )
    .unwrap();
    std::fs::write(root.join("src/lib.rs"), "").unwrap();
    std::fs::write(root.join("locales/en.toml"), "title = \"Settings\"\n").unwrap();
    root
}

fn bake(dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-telar"))
        .args(["telar", "bake"])
        .current_dir(dir)
        .output()
        .expect("cargo-telar starts")
}

#[test]
fn a_catalog_that_does_not_parse_fails_the_bake_and_names_the_file() {
    let root = package("broken_catalog");
    let baked = bake(&root);
    assert!(
        baked.status.success(),
        "a catalog that parses bakes: {}",
        String::from_utf8_lossy(&baked.stderr)
    );

    std::fs::write(root.join("locales/es.toml"), "title = \"Ajustes\n").unwrap();
    let failed = bake(&root);
    let stderr = String::from_utf8_lossy(&failed.stderr);
    assert!(!failed.status.success(), "{stderr}");
    assert!(stderr.contains("error"), "{stderr}");
    assert!(
        stderr.contains(&root.join("locales/es.toml").display().to_string()),
        "{stderr}"
    );

    let _ = std::fs::remove_dir_all(&root);
}
