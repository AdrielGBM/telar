//! `cargo telar test` rendering a component that contains text.
//!
//! The harness exits the process when it is done, so each test runs it in a process of its own: this same binary, started again on that one test, as `cargo telar test` starts the app it builds. A fresh process is also what makes the test honest — a measurer or a face installed by an earlier test in the same process would hide the harness not installing them. The face is declared in the config and checked while the preview is built, which is how `[[telar.fonts]]` reaches the shaper.

use std::path::PathBuf;

use telar::preview::test_run::{CheckStatus, TestReport};
use telar::preview::{Axis, Matrix, Play, PlayResult, PreviewCtx, PreviewEntry};
use telar::{
    AppConfig, Color, Direction, FontAsset, LayoutError, LayoutItem, LayoutStyle, RectStyle,
    StyledContainer, Text, TextStyle, box_item, font_families, font_family_available, try_run_test,
};
use telar_project::protocol::{
    TEST_A11Y_VAR, TEST_PACKAGE_DIR_VAR, TEST_PREVIEW_VAR, TEST_SNAPSHOTS_VAR, TEST_TARGET_DIR_VAR,
};
use test_process::OwnProcess;

const DECLARED_FAMILY: &str = "Preview Harness Face";
const DECLARED_FACE: &[u8] =
    include_bytes!("../../renderer/renderer-text/test-fonts/TelarTest.ttf");

/// Runs the test `own` names in its own process, where the harness exits on its own, and answers whether the harness passed there and what the process printed.
fn harness_in(own: &OwnProcess) -> (bool, String) {
    let output = own
        .command()
        .output()
        .expect("start the test's own process");
    let printed = format!(
        "{}\n--- stderr\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), printed)
}

fn labelled(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    assert!(
        font_family_available(DECLARED_FAMILY),
        "the face the app config declares was not loaded before the preview was built"
    );
    assert!(
        font_families()
            .iter()
            .any(|family| family == DECLARED_FAMILY),
        "and is offered under the family it declares"
    );
    let text = Text::new(
        || String::from("A preview with some text to measure"),
        LayoutStyle::new(),
        || TextStyle::new(14.0, Color::rgba(0.1, 0.1, 0.1, 1.0)),
    )?;
    Ok(Box::new(text))
}

#[test]
fn a_preview_with_text_renders_without_an_installed_measurer() {
    let own = OwnProcess::new(
        module_path!(),
        "a_preview_with_text_renders_without_an_installed_measurer",
    );
    if own.is_current() {
        let entry = PreviewEntry::new(
            "harness--labelled--default",
            "labelled",
            "default",
            labelled,
        );
        let config = AppConfig::default()
            .with_fonts([FontAsset::embedded(DECLARED_FACE).named(DECLARED_FAMILY)]);
        try_run_test(vec![entry], config);
    }

    let (passed, printed) = harness_in(&own);
    assert!(
        passed && printed.contains("1 passed, 0 failed"),
        "the harness failed a preview with text:\n{printed}"
    );
}

fn plain(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(telar::Container::new(
        LayoutStyle::new(),
        Vec::new(),
    )?))
}

#[test]
fn previews_that_share_an_id_fail_the_run() {
    let own = OwnProcess::new(module_path!(), "previews_that_share_an_id_fail_the_run");
    if own.is_current() {
        let entries = vec![
            PreviewEntry::new("harness--plain--twice", "plain", "Twice", plain)
                .location("/a.rs", 1),
            PreviewEntry::new("harness--plain--twice", "plain", "twice", plain)
                .location("/b.rs", 2),
        ];
        try_run_test(entries, AppConfig::default());
    }

    let (passed, printed) = harness_in(&own);
    assert!(!passed, "{printed}");
    assert!(
        printed.contains(
            "FAIL  preview id `harness--plain--twice` names 2 previews (/a.rs:1, /b.rs:2)"
        ),
        "{printed}"
    );
    assert!(printed.contains("2 passed, 1 failed"), "{printed}");
}

const RUN_TEST: &str = "a_test_run_checks_each_preview_and_cell";
const RUN_SET_VAR: &str = "TELAR_PREVIEW_HARNESS_RUN_SET";

fn badge(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = Text::new(
        || String::from("Badge"),
        LayoutStyle::new(),
        || TextStyle::new(14.0, Color::WHITE),
    )?;
    Ok(box_item(StyledContainer::new(
        LayoutStyle::new().padding_all(8.0),
        |_| RectStyle::filled(Color::from_rgb_u8(0x22, 0x55, 0xaa), 4.0),
        vec![box_item(label)],
    )?))
}

fn faint(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = Text::new(
        || String::from("Faint"),
        LayoutStyle::new(),
        || TextStyle::new(14.0, Color::from_rgb_u8(0xee, 0xee, 0xee)),
    )?;
    Ok(box_item(StyledContainer::new(
        LayoutStyle::new().padding_all(8.0),
        |_| RectStyle::filled(Color::WHITE, 0.0),
        vec![box_item(label)],
    )?))
}

fn finds_the_label(canvas: &mut Play) -> PlayResult {
    canvas.expect_text("Badge")
}

fn finds_nothing(canvas: &mut Play) -> PlayResult {
    canvas.expect_text("Absent")
}

const DIRECTIONS: Matrix = Matrix::Axes(&[Axis::Dir(&[Direction::Ltr, Direction::Rtl])]);

/// Which previews a child run tests, by the name the parent passes in `RUN_MARKER`.
fn run_entries(set: &str) -> Vec<PreviewEntry> {
    let badge = PreviewEntry::new("harness--badge--default", "badge", "default", badge)
        .viewport(160.0, 80.0)
        .locale("en")
        .play(finds_the_label)
        .matrix(DIRECTIONS);
    match set {
        "badge" => vec![badge],
        "failing" => vec![
            badge,
            PreviewEntry::new("harness--badge--lost", "badge", "lost", self::badge)
                .viewport(160.0, 80.0)
                .play(finds_nothing),
        ],
        "faint" => vec![
            PreviewEntry::new("harness--faint--default", "faint", "default", faint).locale("en"),
        ],
        other => panic!("no preview set {other}"),
    }
}

/// The process the tests below start, testing the preview set `RUN_SET_VAR` names; anywhere else it tests nothing.
#[test]
fn a_test_run_checks_each_preview_and_cell() {
    if OwnProcess::new(module_path!(), RUN_TEST).is_current() {
        let set = std::env::var(RUN_SET_VAR).expect("the preview set");
        let config = AppConfig::default()
            .with_fonts([FontAsset::embedded(DECLARED_FACE).named(DECLARED_FAMILY)]);
        try_run_test(run_entries(&set), config);
    }
}

struct Run {
    root: PathBuf,
}

impl Run {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "telar_preview_harness_{name}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        Self { root }
    }

    fn package(&self) -> PathBuf {
        self.root.join("package")
    }

    fn target(&self) -> PathBuf {
        self.root.join("target")
    }

    fn report(&self) -> PathBuf {
        self.target().join("telar-snapshots").join("report.json")
    }

    /// Runs the preview `set` in its own process, as `cargo telar test` would with `vars`, and answers whether it passed and what it printed.
    fn test(&self, set: &str, vars: &[(&str, &str)]) -> (bool, String) {
        let mut own = OwnProcess::new(module_path!(), RUN_TEST);
        own.env(RUN_SET_VAR, set)
            .env(TEST_PACKAGE_DIR_VAR, self.package())
            .env(TEST_TARGET_DIR_VAR, self.target());
        for (key, value) in vars {
            own.env(key, value);
        }
        harness_in(&own)
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn snapshots_are_written_on_update_then_matched_cell_by_cell() {
    let run = Run::new("snapshots");
    let (passed, stdout) = run.test("badge", &[(TEST_SNAPSHOTS_VAR, "compare")]);
    assert!(!passed, "a preview with no golden fails:\n{stdout}");
    assert!(stdout.contains("--update-snapshots"), "{stdout}");

    let (passed, stdout) = run.test("badge", &[(TEST_SNAPSHOTS_VAR, "update")]);
    assert!(passed, "{stdout}");
    let goldens = run.package().join("snapshots");
    for id in [
        "harness--badge--default",
        "harness--badge--default--dir=ltr",
        "harness--badge--default--dir=rtl",
    ] {
        assert!(goldens.join(format!("{id}.png")).is_file(), "{id}");
        assert!(goldens.join(format!("{id}.draw.txt")).is_file(), "{id}");
    }

    let (passed, stdout) = run.test("badge", &[(TEST_SNAPSHOTS_VAR, "compare")]);
    assert!(passed, "{stdout}");
    assert!(
        stdout.contains("ok    harness--badge--default--dir=rtl  (")
            && stdout.contains("play: 1 step(s)")
            && stdout.contains("snapshot matched"),
        "{stdout}"
    );
    assert!(stdout.contains("3 passed, 0 failed"), "{stdout}");

    let report = TestReport::read(&run.report()).unwrap();
    assert_eq!(report.snapshots.summary.matched, 3, "{report:#?}");
    assert_eq!(report.test.summary.passed, 3, "{report:#?}");
    let rtl = &report.test.previews[0].cells[2];
    assert_eq!(rtl.id, "harness--badge--default--dir=rtl");
    assert_eq!(rtl.cell, [(String::from("dir"), String::from("rtl"))]);
    assert_eq!(rtl.play.status, CheckStatus::Passed);
    assert_eq!(rtl.snapshot.status, CheckStatus::Passed);
}

#[test]
fn a_failing_play_fails_its_preview_and_the_run() {
    let run = Run::new("play");
    let (passed, stdout) = run.test("failing", &[]);
    assert!(!passed, "{stdout}");
    assert!(stdout.contains("FAIL  harness--badge--lost"), "{stdout}");
    assert!(
        stdout.contains("play: expected the text \"Absent\""),
        "{stdout}"
    );
    assert!(stdout.contains("3 passed, 1 failed"), "{stdout}");
    assert!(
        run.report().is_file(),
        "a run writes its report without snapshots too"
    );
}

#[test]
fn a_filter_narrows_the_run_to_the_ids_it_matches() {
    let run = Run::new("filter");
    let (passed, stdout) = run.test("failing", &[(TEST_PREVIEW_VAR, "*--dir=rtl")]);
    assert!(passed, "{stdout}");
    assert!(stdout.contains("1 passed, 0 failed"), "{stdout}");
    assert!(!stdout.contains("harness--badge--lost"), "{stdout}");

    let (passed, stdout) = run.test("failing", &[(TEST_PREVIEW_VAR, "nothing--*")]);
    assert!(!passed, "{stdout}");
    assert!(
        stdout.contains("no preview or matrix cell id matches `nothing--*`"),
        "{stdout}"
    );
}

#[test]
fn accessibility_errors_warn_unless_asked_to_fail() {
    let run = Run::new("a11y");
    let (passed, stdout) = run.test("faint", &[]);
    assert!(passed, "{stdout}");
    assert!(stdout.contains("warn  harness--faint--default"), "{stdout}");
    assert!(stdout.contains("contrast"), "{stdout}");

    let (passed, stdout) = run.test("faint", &[(TEST_A11Y_VAR, "error")]);
    assert!(!passed, "{stdout}");
    assert!(stdout.contains("FAIL  harness--faint--default"), "{stdout}");

    let (passed, stdout) = run.test("faint", &[(TEST_A11Y_VAR, "off")]);
    assert!(passed, "{stdout}");
    assert!(stdout.contains("ok    harness--faint--default"), "{stdout}");
}
