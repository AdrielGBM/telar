use std::path::PathBuf;

use super::{
    BuildFlavour, PackageError, PackageOptions, generated_dir, transpile_package, write_package,
};

fn package(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_project_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("src")).unwrap();
    root
}

fn options(src_dir: &std::path::Path, flavour: BuildFlavour) -> PackageOptions<'_> {
    PackageOptions {
        src_dir,
        theme_type: None,
        assets: None,
        flavour,
    }
}

/// The output tree mirrors the source tree, so two components may share a basename.
#[test]
fn output_paths_mirror_the_source_tree() {
    let root = package("mirror");
    std::fs::create_dir_all(root.join("src/features")).unwrap();
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();
    std::fs::write(root.join("src/features/home.rsx"), "[view]\ntext \"b\"\n").unwrap();

    let src_dir = root.join("src");
    let files = transpile_package(&options(&src_dir, BuildFlavour::Plain)).unwrap();

    let outs: Vec<_> = files.iter().map(|f| f.rel_out.clone()).collect();
    assert!(outs.contains(&PathBuf::from("home.rs")), "{outs:?}");
    assert!(
        outs.contains(&PathBuf::from("features").join("home.rs")),
        "{outs:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// The flavour is what the caller passed, not what the environment happened to hold: the same source transpiled twice differs only because the two asked for different things.
#[test]
fn the_hot_flavour_keys_signals_and_the_plain_one_does_not() {
    let root = package("flavour");
    std::fs::write(
        root.join("src/counter.rsx"),
        "[logic]\nlet count = signal(0);\n\n[view]\ntext \"{$count}\"\n",
    )
    .unwrap();

    let src_dir = root.join("src");
    let plain = transpile_package(&options(&src_dir, BuildFlavour::Plain)).unwrap();
    let hot = transpile_package(&options(&src_dir, BuildFlavour::Hot)).unwrap();

    assert!(
        !plain[0].source.rust_code.contains("hot_signal_auto!"),
        "{}",
        plain[0].source.rust_code
    );
    assert!(
        hot[0].source.rust_code.contains("hot_signal_auto!"),
        "{}",
        hot[0].source.rust_code
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// The two flavours are different Rust for one source, so they cannot share a directory.
#[test]
fn each_flavour_writes_its_own_directory() {
    let root = package("dirs");
    assert_eq!(
        generated_dir(&root, BuildFlavour::Plain),
        root.join(".telar").join("build")
    );
    assert_eq!(
        generated_dir(&root, BuildFlavour::Hot),
        root.join(".telar").join("build-hot")
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// A parse error names the `.rsx` and the line, because the macro relays this text verbatim and a `compile_error!` pointing at generated Rust is what the source map exists to undo.
#[test]
fn a_parse_error_names_the_file_it_came_from() {
    let root = package("parse_error");
    std::fs::write(
        root.join("src/broken.rsx"),
        "text \"stray\"\n\n[view]\ntext \"x\"\n",
    )
    .unwrap();

    let src_dir = root.join("src");
    let err = transpile_package(&options(&src_dir, BuildFlavour::Plain)).unwrap_err();

    assert!(matches!(err, PackageError::Parse { .. }), "{err:?}");
    assert!(err.to_string().contains("broken.rsx"), "{err}");

    let _ = std::fs::remove_dir_all(&root);
}

/// Every written path comes back, so what a renamed or deleted `.rsx` left behind can be told from live output.
#[test]
fn writing_reports_what_it_wrote() {
    let root = package("written");
    std::fs::write(root.join("src/home.rsx"), "[view]\ntext \"a\"\n").unwrap();

    let src_dir = root.join("src");
    let files = transpile_package(&options(&src_dir, BuildFlavour::Plain)).unwrap();
    let out_dir = generated_dir(&root, BuildFlavour::Plain);
    let written = write_package(&files, &out_dir).unwrap();

    assert_eq!(written, [out_dir.join("home.rs")].into_iter().collect());
    assert!(out_dir.join("home.rs").is_file());
    assert!(
        out_dir.join("home.rs.map").is_file(),
        "the map is what puts a diagnostic back on the .rsx line"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// `DIR_NAMES` is read by `cargo-telar` to recognise a generated path, and `dir_name` is what writes one. Two lists of the same thing, so a fifth flavour added to one and not the other makes every diagnostic in it point at generated Rust instead of at the `.rsx`.
#[test]
fn every_flavour_writes_a_directory_the_list_names() {
    for flavour in BuildFlavour::ALL {
        assert!(
            BuildFlavour::DIR_NAMES.contains(&flavour.dir_name()),
            "{flavour:?} writes `{}`, which nothing downstream recognises",
            flavour.dir_name()
        );
    }
    let mut names: Vec<&str> = BuildFlavour::ALL.iter().map(|f| f.dir_name()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(
        names.len(),
        BuildFlavour::DIR_NAMES.len(),
        "one directory per flavour, and no two sharing one"
    );
}

/// The pair a flavour stands for, round-tripped: a caller builds one from two booleans and reads them back off it.
#[test]
fn a_flavour_reports_the_pair_it_was_built_from() {
    for hot in [false, true] {
        for previews in [false, true] {
            let flavour = BuildFlavour::new(hot, previews);
            assert_eq!(flavour.is_hot(), hot, "{flavour:?}");
            assert_eq!(flavour.has_previews(), previews, "{flavour:?}");
        }
    }
}

/// The whole point of the flavour: a build that does not ask for previews gets none generated.
///
/// Not a size optimisation. These fns used to be emitted into every build, `telar_all_preview_entries` named them from the crate root, and `telar::dev_entry` called that from the `run()` of every application — so the markup of every `[preview]` block was live code in a shipped binary, and `TELAR_TEST=1 ./app` ran the preview harness and exited for anyone who set it.
#[test]
fn a_flavour_without_previews_generates_none() {
    let root = package("no_previews");
    let src = root.join("src");
    std::fs::write(
        src.join("card.rsx"),
        "[view]\ntext \"body\"\n\n[preview \"Default\"]\ncard\n",
    )
    .unwrap();

    let with = transpile_package(&options(&src, BuildFlavour::Preview)).unwrap();
    let without = transpile_package(&options(&src, BuildFlavour::Plain)).unwrap();

    assert!(
        with[0].source.rust_code.contains("card_preview_0"),
        "the preview flavour emits a build fn per block:\n{}",
        with[0].source.rust_code
    );
    assert_eq!(with[0].source.preview_names, ["Default"]);

    assert!(
        !without[0].source.rust_code.contains("preview"),
        "a plain build must carry no trace of one:\n{}",
        without[0].source.rust_code
    );
    assert!(
        without[0].source.preview_names.is_empty(),
        "and must not report previews a reader would then wire to nothing"
    );
    assert!(without[0].source.rust_code.contains("pub fn card("));
}
