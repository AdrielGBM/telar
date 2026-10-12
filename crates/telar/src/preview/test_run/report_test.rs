use super::*;
use crate::Rect;
use crate::preview::a11y::{Rule, Violation};

use CheckStatus::{Failed, Passed, Skipped, Warned};

fn cell(id: &str, statuses: [CheckStatus; 4]) -> CellReport {
    let [build, play, a11y, snapshot] = statuses;
    let mut cell = CellReport::new(id.to_string(), Vec::new());
    cell.build.status = build;
    cell.play.status = play;
    cell.a11y.status = a11y;
    cell.snapshot.status = snapshot;
    cell.settle()
}

#[test]
fn a_cell_passes_unless_one_of_its_checks_failed() {
    assert!(cell("a", [Passed, Skipped, Warned, Passed]).passed);
    assert!(!cell("b", [Passed, Failed, Passed, Passed]).passed);
    assert!(!cell("c", [Passed, Passed, Passed, Failed]).passed);
    assert!(!cell("d", [Failed, Skipped, Skipped, Skipped]).passed);
}

#[test]
fn the_summary_counts_cells_and_every_error_outside_them() {
    let mut run = TestRun {
        errors: vec![String::from("preview id `x` names 2 previews")],
        previews: vec![PreviewReport {
            id: String::from("demo--card--a"),
            cells: vec![
                cell("demo--card--a", [Passed, Passed, Warned, Passed]),
                cell("demo--card--a--mode=dark", [Passed, Failed, Passed, Passed]),
            ],
            ..PreviewReport::default()
        }],
        ..TestRun::default()
    };
    run.summarize();
    assert_eq!(
        run.summary,
        TestSummary {
            previews: 1,
            checked: 2,
            passed: 1,
            failed: 2,
            warned: 1,
        }
    );
    assert!(!run.passed());
}

#[test]
fn the_details_name_the_check_each_line_comes_from() {
    let mut cell = CellReport::new(String::from("demo--card--a"), Vec::new());
    cell.play.error = Some(String::from("found no button \"Save\""));
    cell.a11y.violations = vec![A11yFinding::from(&Violation::new(
        Rule::Contrast,
        "text at 2.1:1",
        Some(Rect::new(1.0, 2.0, 30.0, 12.0)),
    ))];
    cell.snapshot = SnapshotCheck {
        status: CheckStatus::Passed,
        detail: Some(String::from("matched")),
    };
    let details = cell.details();
    assert_eq!(details.len(), 2, "{details:?}");
    assert_eq!(details[0], "play: found no button \"Save\"");
    assert!(details[1].starts_with("a11y: error"), "{}", details[1]);
    assert!(details[1].contains("contrast"), "{}", details[1]);
    assert_eq!(cell.a11y.violations[0].rect, Some([1.0, 2.0, 30.0, 12.0]));

    cell.snapshot.status = CheckStatus::Failed;
    assert_eq!(cell.details().last().unwrap(), "snapshot: matched");
}

#[test]
fn the_written_report_still_reads_as_a_snapshot_report() {
    use crate::preview::snapshot::{SnapshotMode, SnapshotPaths, SnapshotReport, Tolerance};

    let root = std::env::temp_dir().join(format!("telar_test_report_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let paths = SnapshotPaths::new(&root.join("package"), &root.join("target"));
    let snapshots = SnapshotReport {
        version: SnapshotReport::VERSION,
        mode: SnapshotMode::Compare,
        goldens: paths.goldens.clone(),
        out: paths.out.clone(),
        tolerance: Tolerance::default(),
        summary: Default::default(),
        snapshots: Vec::new(),
    };
    let mut test = TestRun {
        a11y: A11yLevel::Error,
        snapshots: SnapshotSetting::Compare,
        previews: vec![PreviewReport {
            id: String::from("demo--card--a"),
            cells: vec![cell("demo--card--a", [Passed, Skipped, Passed, Passed])],
            ..PreviewReport::default()
        }],
        ..TestRun::default()
    };
    test.summarize();
    let report = TestReport::new(snapshots.clone(), test);
    report.write(&paths.report()).unwrap();

    assert_eq!(TestReport::read(&paths.report()).unwrap(), report);
    assert_eq!(SnapshotReport::read(&paths.report()).unwrap(), snapshots);
    let json = std::fs::read_to_string(paths.report()).unwrap();
    assert!(json.contains("\"test\": {"), "{json}");
    assert!(json.contains("\"a11y\": \"error\""), "{json}");
    assert!(json.contains("\"status\": \"skipped\""), "{json}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn accepting_a_drift_writes_both_goldens_and_passes_its_cell() {
    use crate::preview::snapshot::{
        DrawCheck, FileStatus, PngCheck, SnapshotError, SnapshotMode, SnapshotPaths,
        SnapshotReport, SnapshotResult, SnapshotStatus, SnapshotSummary, Tolerance,
    };

    let root =
        std::env::temp_dir().join(format!("telar_test_report_accept_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let paths = SnapshotPaths::new(&root.join("package"), &root.join("target"));
    let id = "demo--card--a--mode=dark";
    let captured = |suffix: &str| paths.out.join(format!("{id}{suffix}"));
    std::fs::create_dir_all(&paths.out).unwrap();
    std::fs::create_dir_all(&paths.goldens).unwrap();
    std::fs::write(captured(".png"), b"the picture captured").unwrap();
    std::fs::write(captured(".draw.txt"), "frame 160x80\n").unwrap();
    std::fs::write(captured(".diff.png"), b"where it differs").unwrap();
    std::fs::write(paths.golden_png(id), b"the old picture").unwrap();
    std::fs::write(paths.golden_draw(id), "frame 100x80\n").unwrap();
    let drifted = SnapshotResult {
        id: id.to_string(),
        preview: String::from("demo--card--a"),
        cell: vec![(String::from("mode"), String::from("dark"))],
        status: SnapshotStatus::Changed,
        png: Some(PngCheck {
            status: FileStatus::Changed,
            golden: paths.golden_png(id),
            actual: Some(captured(".png")),
            diff: Some(captured(".diff.png")),
            size: [160, 80],
            golden_size: Some([100, 80]),
            differing_pixels: 3,
            max_channel_delta: 255,
        }),
        draw: Some(DrawCheck {
            status: FileStatus::Changed,
            golden: paths.golden_draw(id),
            actual: Some(captured(".draw.txt")),
            diff: None,
        }),
        error: None,
    };
    let snapshots = vec![drifted];
    let snapshot_report = SnapshotReport {
        version: SnapshotReport::VERSION,
        mode: SnapshotMode::Compare,
        goldens: paths.goldens.clone(),
        out: paths.out.clone(),
        tolerance: Tolerance::default(),
        summary: SnapshotSummary::of(&snapshots),
        snapshots,
    };
    let mut test = TestRun {
        previews: vec![PreviewReport {
            id: String::from("demo--card--a"),
            cells: vec![
                cell("demo--card--a", [Passed, Skipped, Passed, Passed]),
                cell(id, [Passed, Skipped, Passed, Failed]),
            ],
            ..PreviewReport::default()
        }],
        ..TestRun::default()
    };
    test.summarize();
    let mut report = TestReport::new(snapshot_report, test);
    assert!(report.snapshots.snapshots[0].acceptable());

    report.accept(id).expect("a drifted snapshot is accepted");

    assert_eq!(
        std::fs::read(paths.golden_png(id)).unwrap(),
        b"the picture captured"
    );
    assert_eq!(
        std::fs::read_to_string(paths.golden_draw(id)).unwrap(),
        "frame 160x80\n"
    );
    for suffix in [".png", ".draw.txt", ".diff.png"] {
        assert!(
            !captured(suffix).exists(),
            "{suffix} goes once it is the golden"
        );
    }
    let accepted = &report.snapshots.snapshots[0];
    assert_eq!(accepted.status, SnapshotStatus::Updated);
    assert_eq!(accepted.png.as_ref().unwrap().status, FileStatus::Written);
    assert_eq!(accepted.draw.as_ref().unwrap().status, FileStatus::Written);
    assert!(!accepted.acceptable());
    assert!(report.snapshots.passed());
    assert_eq!(report.snapshots.summary.updated, 1);
    let cell = &report.test.previews[0].cells[1];
    assert!(cell.passed);
    assert_eq!(cell.snapshot.detail.as_deref(), Some("written"));
    assert!(report.test.passed(), "{:?}", report.test.summary);

    assert!(matches!(
        report.accept(id),
        Err(SnapshotError::NothingToAccept(_))
    ));
    assert!(matches!(
        report.accept("demo--card--b"),
        Err(SnapshotError::NotInReport(_))
    ));
    let _ = std::fs::remove_dir_all(&root);
}
