//! `cargo telar test -p <library>` finds the previews test `rsx_modules!` emits wherever the macro is called: from a nested `mod.rs`, which puts the test under a module path of its own, and from several modules at once, which run the previews and write the report once.
//!
//! It compiles telar with its previews into its own target directory under `target/tmp`, so it is ignored by default: run it with `cargo test -p cargo-telar --test library_previews -- --ignored`. Every cargo it starts runs offline, so the registry has to hold what `Cargo.lock` names first (`cargo fetch`).

use std::path::{Path, PathBuf};
use std::process::Command;

use telar_project::protocol::{TEST_OUT_DIR, TEST_REPORT_FILE};

#[test]
#[ignore = "builds telar with its previews: run it with `cargo test -p cargo-telar --test library_previews -- --ignored`"]
fn a_library_placing_its_rsx_from_a_nested_mod_rs_has_its_previews_tested() {
    let run = test_library("nested-previews", &[("src/ui/mod.rs", RSX_MODULES)]);
    assert_eq!(
        run.preview_ids,
        ["nested_previews--badge--plain"],
        "{}",
        run.shown
    );
}

#[test]
#[ignore = "builds telar with its previews: run it with `cargo test -p cargo-telar --test library_previews -- --ignored`"]
fn a_library_calling_rsx_modules_from_several_modules_runs_its_previews_once() {
    let run = test_library(
        "several-calls",
        &[
            ("src/lib.rs", "telar::rsx_modules!();\nmod ui;\n"),
            ("src/ui/mod.rs", RSX_MODULES),
        ],
    );
    assert_eq!(
        run.preview_ids,
        ["several_calls--badge--plain"],
        "{}",
        run.shown
    );
    assert_eq!(
        run.shown.matches("running 1 preview(s)").count(),
        1,
        "{}",
        run.shown
    );
}

const RSX_MODULES: &str = "telar::rsx_modules!();\n";

struct LibraryRun {
    preview_ids: Vec<String>,
    shown: String,
}

fn test_library(name: &str, sources: &[(&str, &str)]) -> LibraryRun {
    let repo = repo_root();
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("library-previews");
    let target_dir = work.join(format!("target-{name}"));
    let library = work.join(name);
    if library.exists() {
        std::fs::remove_dir_all(&library).expect("remove the previous run");
    }
    write_library(&library, &repo, name, sources);

    let output = Command::new(env!("CARGO_BIN_EXE_cargo-telar"))
        .args(["test", "--no-snapshots"])
        .current_dir(&library)
        .env("CARGO_TARGET_DIR", &target_dir)
        .env("CARGO_NET_OFFLINE", "true")
        .output()
        .expect("could not start cargo-telar");
    let shown = format!(
        "--- stdout\n{}\n--- stderr\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "cargo telar test failed with {}\n{shown}",
        output.status
    );

    let report_path = target_dir.join(TEST_OUT_DIR).join(TEST_REPORT_FILE);
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).unwrap_or_else(|e| {
            panic!("the run wrote no {}: {e}\n{shown}", report_path.display())
        }))
        .expect("the report is JSON");
    let preview_ids = report["test"]["previews"]
        .as_array()
        .expect("the report lists its previews")
        .iter()
        .filter_map(|preview| preview["id"].as_str().map(str::to_string))
        .collect();
    LibraryRun { preview_ids, shown }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("cargo-telar sits three levels below the repository root")
        .to_path_buf()
}

fn write_library(dir: &Path, repo: &Path, name: &str, sources: &[(&str, &str)]) {
    let manifest = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
telar = {{ path = {telar}, default-features = false, features = ["runtime", "shaper"] }}

[workspace]
"#,
        telar = toml::Value::String(repo.join("crates/telar").display().to_string()),
    );
    let mut files = vec![
        ("Cargo.toml", manifest.as_str()),
        ("telar.toml", "[telar]\nlibrary = true\n"),
        ("src/lib.rs", "mod ui;\n"),
        ("src/ui/badge.rsx", BADGE_RSX),
    ];
    for (path, contents) in sources {
        files.retain(|(existing, _)| existing != path);
        files.push((path, contents));
    }
    for (relative, contents) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a file has a parent"))
            .expect("create a library directory");
        std::fs::write(&path, contents).expect("write a library file");
    }
    // Offline, the versions the workspace already fetched are the only ones the library can resolve to.
    std::fs::copy(repo.join("Cargo.lock"), dir.join("Cargo.lock")).expect("copy Cargo.lock");
}

const BADGE_RSX: &str = r#"[view]
text "Nested" font_size:14

[previews "Nested/Badge"]
A badge placed from a nested `mod.rs`.

[preview "Plain"]
badge
"#;
