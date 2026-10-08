use std::path::PathBuf;

use super::{PackageError, PackageOptions, transpile_package, write_package};
use telar_project::{BuildFlavour, generated_dir};

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
        prelude: &[],
        library: false,
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

const BROKEN_VIEW: &str = "[view]\ncol\n    text \"fine\"\n    text \"oops\n";

/// The point of carrying the path in the error: a macro or a CLI relaying it names the `.rsx` and the line, which is all the author has to go on.
#[test]
fn a_refused_source_is_named_with_its_file_and_line() {
    let root = package("refused");
    std::fs::write(root.join("src/ok.rsx"), "[view]\ntext \"a\"\n").unwrap();
    std::fs::write(root.join("src/broken.rsx"), BROKEN_VIEW).unwrap();

    let src_dir = root.join("src");
    let error = transpile_package(&options(&src_dir, BuildFlavour::Plain)).unwrap_err();

    let message = error.to_string();
    assert!(
        message.starts_with(&format!("{}:", root.join("src/broken.rsx").display())),
        "{message}"
    );
    assert_eq!(error.path(), root.join("src/broken.rsx"));

    let _ = std::fs::remove_dir_all(&root);
}

/// What the macro reads back: the message as printed, and the hash that lets it tell a standing failure from one the author has since fixed.
#[test]
fn a_failure_record_holds_only_while_its_source_is_unchanged() {
    let root = package("record");
    std::fs::write(root.join("src/broken.rsx"), BROKEN_VIEW).unwrap();
    let src_dir = root.join("src");

    let error = transpile_package(&options(&src_dir, BuildFlavour::Plain)).unwrap_err();
    let failure = error.to_build_failure(&src_dir);
    assert_eq!(failure.source, "broken.rsx");
    assert_eq!(failure.message, error.to_string());
    assert!(failure.still_applies(&src_dir));

    std::fs::write(root.join("src/broken.rsx"), "[view]\ntext \"fixed\"\n").unwrap();
    assert!(!failure.still_applies(&src_dir));

    let _ = std::fs::remove_dir_all(&root);
}

/// A failure that outlives its cause would be the macro blaming a file for an error it no longer has.
#[test]
fn a_recorded_failure_is_forgotten_when_cleared() {
    let root = package("clear");
    let failure = telar_project::BuildFailure {
        source: "a.rsx".to_string(),
        hash: "h".to_string(),
        message: "boom".to_string(),
    };
    telar_project::write_build_failure(&root, BuildFlavour::Plain, &failure).unwrap();
    assert_eq!(
        telar_project::read_build_failure(&root, BuildFlavour::Plain),
        Some(failure)
    );
    assert_eq!(
        telar_project::read_build_failure(&root, BuildFlavour::Hot),
        None
    );

    telar_project::clear_build_failure(&root, BuildFlavour::Plain);
    assert_eq!(
        telar_project::read_build_failure(&root, BuildFlavour::Plain),
        None
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// Each prelude entry is glob-imported between `telar`'s glob and the crate's own, in the order declared, and an explicit `use` from `[logic]` still lands after all three — where it shadows any of them.
#[test]
fn the_prelude_is_imported_between_telar_and_the_crate() {
    let root = package("prelude");
    std::fs::write(
        root.join("src/home.rsx"),
        "[logic]\nuse other::button;\n\n[view]\nbutton\n",
    )
    .unwrap();
    let prelude = [
        telar_project::PreludeEntry::parse("telar-components").unwrap(),
        telar_project::PreludeEntry::parse("my_plugin::prelude").unwrap(),
    ];
    let src_dir = root.join("src");
    let files = transpile_package(&PackageOptions {
        prelude: &prelude,
        library: false,
        ..options(&src_dir, BuildFlavour::Plain)
    })
    .unwrap();
    let code = &files[0].source.rust_code;

    let header: Vec<&str> = code.lines().filter(|line| line.contains(" use ")).collect();
    assert_eq!(
        header,
        [
            "#[allow(unused_imports)] use telar::*;",
            "#[allow(unused_imports)] use telar_components::*;",
            "#[allow(unused_imports)] use my_plugin::prelude::*;",
            "#[allow(unused_imports)] use crate::*;",
            "#[allow(unused_imports)] use other::button;",
        ],
        "{code}"
    );
    let map = &files[0].source.source_map;
    let line_of = |needle: &str| code.lines().position(|line| line.contains(needle)).unwrap();
    assert_eq!(map[line_of("use telar_components::*")], None);
    assert_eq!(map[line_of("use other::button")], Some(1));

    let _ = std::fs::remove_dir_all(&root);
}

/// The editor's mirror and the package walk are one code path, so a buffer transpiled in memory is byte-identical to the same text transpiled from disk — prelude included.
#[test]
fn a_buffer_transpiles_as_the_saved_file_would() {
    let root = package("buffer");
    let source = "[view]\ntext \"a\"\n";
    let rsx = root.join("src/home.rsx");
    std::fs::write(&rsx, source).unwrap();
    let prelude = [telar_project::PreludeEntry::parse("telar-components").unwrap()];
    let src_dir = root.join("src");
    let options = PackageOptions {
        prelude: &prelude,
        library: false,
        ..options(&src_dir, BuildFlavour::Preview)
    };

    let walked = transpile_package(&options).unwrap();
    let buffered = super::transpile_buffer(&rsx, source, &options).unwrap();
    assert_eq!(walked[0].source.rust_code, buffered.rust_code);
    assert!(buffered.rust_code.contains("use telar_components::*;"));

    let _ = std::fs::remove_dir_all(&root);
}

/// A previews file is accepted beside the component it previews, and registers nothing the macro would wire to a module the tree never declares.
#[test]
fn a_previews_file_is_accepted_and_a_view_in_one_is_refused() {
    let root = package("previews_file");
    let src = root.join("src");
    std::fs::write(src.join("card.rsx"), "[view]\ntext \"body\"\n").unwrap();
    std::fs::write(
        src.join("card.previews.rsx"),
        "[previews \"Cards/Card\"]\nA card.\n\n[preview \"Default\" args(title:\"Hi\")]\ncard\n\n[play]\ncanvas.expect_text(\"Hi\")?;\n",
    )
    .unwrap();

    let files = transpile_package(&options(&src, BuildFlavour::Preview)).unwrap();
    let previews = files
        .iter()
        .find(|file| file.rel_out == std::path::Path::new("card.previews.rs"))
        .expect("the previews file is transpiled");
    assert!(previews.source.preview_names.is_empty());
    assert!(!previews.source.rust_code.contains("fn "));

    std::fs::write(src.join("card.previews.rsx"), "[view]\ncol\n").unwrap();
    let error = transpile_package(&options(&src, BuildFlavour::Preview)).unwrap_err();
    assert!(error.to_string().contains("no `[view]`"), "{error}");

    let _ = std::fs::remove_dir_all(&root);
}
