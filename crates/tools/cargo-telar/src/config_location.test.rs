//! `telar.toml` as the one place a project is configured, seen through `cargo telar` the way a user runs it: a `[package.metadata.telar]` table left in `Cargo.toml` is reported, never read.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn package_with_cargo_metadata(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "cargo_telar_config_location_{name}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n\n[package.metadata.telar]\nbackend = \"software\"\n",
    )
    .unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    root
}

fn cargo_telar(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-telar"))
        .arg("telar")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("cargo-telar starts")
}

#[test]
fn doctor_reports_a_telar_table_in_cargo_metadata_as_an_error_naming_telar_toml() {
    let root = package_with_cargo_metadata("doctor");
    let output = cargo_telar(&root, &["doctor"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let reported = stdout
        .lines()
        .find(|line| line.contains("\u{2717} configuration:"))
        .unwrap_or_else(|| panic!("no configuration error reported:\n{stdout}"));
    assert!(reported.contains("[package.metadata.telar]"), "{reported}");
    assert!(reported.contains("telar.toml"), "{reported}");
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_build_refuses_a_telar_table_in_cargo_metadata_naming_telar_toml() {
    let root = package_with_cargo_metadata("build");
    let output = cargo_telar(&root, &["build"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("[package.metadata.telar]"), "{stderr}");
    assert!(stderr.contains("telar.toml"), "{stderr}");
    assert!(!root.join("target").exists(), "nothing was built: {stderr}");
    let _ = std::fs::remove_dir_all(&root);
}
