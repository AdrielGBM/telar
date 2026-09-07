use std::path::PathBuf;

use crate::{PackageOptions, build_index, transpile_package, write_package};
use telar_project::{
    BUILD_ARTIFACT_FORMAT, BuildFlavour, generated_dir, read_build_index, write_build_index,
};

fn package(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_artifact_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    root
}

fn transpile(root: &std::path::Path) -> super::BuildIndex {
    let src_dir = root.join("src");
    let files = transpile_package(&PackageOptions {
        src_dir: &src_dir,
        theme_type: Some("app::Theme"),
        assets: None,
        flavour: BuildFlavour::Plain,
    })
    .unwrap();
    write_package(&files, &generated(root)).unwrap();
    build_index(&files, &src_dir, Some("app::Theme"), "test", "0.0.0")
}

fn generated(root: &std::path::Path) -> PathBuf {
    generated_dir(root, BuildFlavour::Plain)
}

/// The whole point: an artifact that still describes the sources is one the macro may wire instead of producing again.
#[test]
fn an_untouched_package_is_answered_for() {
    let root = package("fresh");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();
    let index = transpile(&root);

    assert!(index.answers_for(
        &root.join("src"),
        &generated(&root),
        Some("app::Theme"),
        "0.0.0"
    ));

    let _ = std::fs::remove_dir_all(&root);
}

/// The failure this exists to prevent: markup edited after a transpile, compiled from the output of the one before it, silently.
#[test]
fn an_edited_source_is_not() {
    let root = package("edited");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();
    let index = transpile(&root);
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"b\"\n").unwrap();

    assert!(!index.answers_for(
        &root.join("src"),
        &generated(&root),
        Some("app::Theme"),
        "0.0.0"
    ));

    let _ = std::fs::remove_dir_all(&root);
}

/// A file added since has no generated Rust to wire, and one deleted since would be wired to output about to be pruned. Neither shows up as a changed hash, so the set is compared too.
#[test]
fn a_source_added_or_removed_since_is_not() {
    let root = package("set");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();
    let index = transpile(&root);

    std::fs::write(root.join("src/other.rsx"), "[view]\ntext \"b\"\n").unwrap();
    assert!(
        !index.answers_for(
            &root.join("src"),
            &generated(&root),
            Some("app::Theme"),
            "0.0.0"
        ),
        "a new .rsx has no output yet"
    );

    std::fs::remove_file(root.join("src/other.rsx")).unwrap();
    std::fs::remove_file(root.join("src/home.rsx")).unwrap();
    assert!(
        !index.answers_for(
            &root.join("src"),
            &generated(&root),
            Some("app::Theme"),
            "0.0.0"
        ),
        "a deleted .rsx leaves output that is about to be swept"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// A different theme is different Rust for the same markup, so the artifact cannot answer for it.
#[test]
fn another_theme_is_not_answered_for() {
    let root = package("theme");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();
    let index = transpile(&root);

    assert!(!index.answers_for(
        &root.join("src"),
        &generated(&root),
        Some("other::Theme"),
        "0.0.0"
    ));
    assert!(!index.answers_for(&root.join("src"), &generated(&root), None, "0.0.0"));

    let _ = std::fs::remove_dir_all(&root);
}

/// Both halves of the handshake the asset artifact runs, for the same reasons.
#[test]
fn a_foreign_format_or_version_is_not_answered_for() {
    let root = package("handshake");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();
    let mut index = transpile(&root);

    assert!(!index.answers_for(
        &root.join("src"),
        &generated(&root),
        Some("app::Theme"),
        "0.0.1"
    ));

    index.format = BUILD_ARTIFACT_FORMAT + 1;
    assert!(!index.answers_for(
        &root.join("src"),
        &generated(&root),
        Some("app::Theme"),
        "0.0.0"
    ));

    let _ = std::fs::remove_dir_all(&root);
}

/// Each flavour keeps its own index beside its own directory, or the two would answer for each other's output.
#[test]
fn the_two_flavours_do_not_read_each_others_index() {
    let root = package("flavours");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();
    let index = transpile(&root);
    write_build_index(&root, BuildFlavour::Plain, &index).unwrap();

    assert!(read_build_index(&root, BuildFlavour::Plain).is_some());
    assert!(read_build_index(&root, BuildFlavour::Hot).is_none());

    let _ = std::fs::remove_dir_all(&root);
}

/// A truncated or hand-edited index reads as absent: the reader's answer to both is to transpile for itself.
#[test]
fn a_corrupt_index_reads_as_absent() {
    let root = package("corrupt");
    std::fs::create_dir_all(root.join(".telar")).unwrap();
    std::fs::write(root.join(".telar/build.json"), "{ not json").unwrap();

    assert!(read_build_index(&root, BuildFlavour::Plain).is_none());

    let _ = std::fs::remove_dir_all(&root);
}

/// Output deleted by hand leaves an index that still describes the sources perfectly. Wiring it would declare a `#[path] mod` at a file that is not there, which rustc reports against generated code nobody wrote.
#[test]
fn output_deleted_since_is_not_answered_for() {
    let root = package("no_output");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();
    let index = transpile(&root);
    std::fs::remove_file(generated(&root).join("home.rs")).unwrap();

    assert!(!index.answers_for(
        &root.join("src"),
        &generated(&root),
        Some("app::Theme"),
        "0.0.0"
    ));

    let _ = std::fs::remove_dir_all(&root);
}

/// The editor mirrors an *unsaved* buffer into this same directory, so output that no longer matches what the transpile produced is output the build must not compile on the strength of a source hash that still matches.
#[test]
fn output_rewritten_since_is_not_answered_for() {
    let root = package("overwritten");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();
    let index = transpile(&root);
    let output = generated(&root).join("home.rs");
    let mirrored = format!(
        "{}\n// what the editor is showing\n",
        std::fs::read_to_string(&output).unwrap()
    );
    std::fs::write(&output, mirrored).unwrap();

    assert!(!index.answers_for(
        &root.join("src"),
        &generated(&root),
        Some("app::Theme"),
        "0.0.0"
    ));

    let _ = std::fs::remove_dir_all(&root);
}
