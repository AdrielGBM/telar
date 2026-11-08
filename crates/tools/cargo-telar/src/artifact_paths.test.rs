//! `cargo telar build` run as a user runs it, on a package of its own outside this workspace whose targets are not named after it: what it packages is what cargo reported building.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn package(name: &str, manifest_tail: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "cargo_telar_artifact_paths_{name}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n\n{manifest_tail}",
            name.replace('_', "-")
        ),
    )
    .unwrap();
    std::fs::write(root.join("src/lib.rs"), "pub fn answer() -> u32 { 42 }\n").unwrap();
    std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    root
}

fn build(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cargo-telar"))
        .args(["telar", "build"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("cargo-telar starts")
}

#[test]
fn a_package_whose_lib_and_bin_are_named_otherwise_packages_the_binary_cargo_built() {
    let root = package(
        "renamed_targets",
        "[lib]\nname = \"other\"\ncrate-type = [\"cdylib\", \"rlib\"]\n\n[[bin]]\nname = \"launcher\"\npath = \"src/main.rs\"\n",
    );
    let built = build(&root, &[]);
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(built.status.success(), "{stderr}");
    assert!(
        root.join("target/telar-dist/renamed-targets/renamed-targets")
            .is_file(),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A `--target` passed through to cargo moves the build under `target/<triple>/`, which is where the binary is then found.
#[test]
fn a_target_passed_through_to_cargo_is_where_the_binary_is_found() {
    let root = package("explicit_target", "");
    let host = host_triple();
    let built = build(&root, &["--", "--target", &host]);
    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(built.status.success(), "{stderr}");
    assert!(
        root.join("target").join(&host).join("release").is_dir(),
        "{stderr}"
    );
    assert!(
        root.join("target/telar-dist/explicit-target/explicit-target")
            .is_file(),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

fn host_triple() -> String {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .expect("rustc starts");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("rustc -vV names the host")
        .to_string()
}
