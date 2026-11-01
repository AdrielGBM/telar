use std::path::PathBuf;

use super::*;
use crate::preview::snapshot::{DrawCheck, PngCheck};

fn one(result: &SnapshotResult) -> SnapshotCheck {
    describe([result])
}

fn result(status: SnapshotStatus, png: FileStatus, draw: FileStatus) -> SnapshotResult {
    SnapshotResult {
        id: String::from("demo--card--a"),
        preview: String::from("demo--card--a"),
        subject: String::from("demo--card--a"),
        cell: Vec::new(),
        moment: Moment::End,
        status,
        png: Some(PngCheck {
            status: png,
            golden: PathBuf::from("/goldens/demo--card--a.png"),
            actual: None,
            diff: Some(PathBuf::from("/out/demo--card--a.diff.png")),
            size: [160, 80],
            golden_size: Some([160, 80]),
            differing_pixels: 312,
            max_channel_delta: 64,
        }),
        draw: Some(DrawCheck {
            status: draw,
            golden: PathBuf::from("/goldens/demo--card--a.draw.txt"),
            actual: None,
            diff: Some(PathBuf::from("/out/demo--card--a.draw.diff")),
        }),
        error: None,
    }
}

#[test]
fn a_match_and_an_update_pass() {
    let matched = one(&result(
        SnapshotStatus::Matched,
        FileStatus::Matched,
        FileStatus::Matched,
    ));
    assert_eq!(matched.status, CheckStatus::Passed);
    assert_eq!(matched.detail.as_deref(), Some("matched"));
    let written = one(&result(
        SnapshotStatus::Updated,
        FileStatus::Written,
        FileStatus::Written,
    ));
    assert_eq!(written.status, CheckStatus::Passed);
}

#[test]
fn a_missing_golden_fails_naming_the_file_and_how_to_write_it() {
    let missing = one(&result(
        SnapshotStatus::Missing,
        FileStatus::Matched,
        FileStatus::Missing,
    ));
    assert_eq!(missing.status, CheckStatus::Failed);
    let detail = missing.detail.unwrap();
    assert!(
        detail.contains("/goldens/demo--card--a.draw.txt"),
        "{detail}"
    );
    assert!(!detail.contains(".png"), "{detail}");
    assert!(detail.contains("--update-snapshots"), "{detail}");
}

#[test]
fn a_drift_fails_with_what_moved_and_where_the_diffs_are() {
    let changed = one(&result(
        SnapshotStatus::Changed,
        FileStatus::Changed,
        FileStatus::Changed,
    ));
    assert_eq!(changed.status, CheckStatus::Failed);
    let detail = changed.detail.unwrap();
    assert!(detail.contains("312 pixel(s)"), "{detail}");
    assert!(detail.contains("up to 64"), "{detail}");
    assert!(detail.contains("/out/demo--card--a.diff.png"), "{detail}");
    assert!(detail.contains("the draw text changed"), "{detail}");
    assert!(detail.contains("/out/demo--card--a.draw.diff"), "{detail}");
}

#[test]
fn a_resized_picture_says_both_sizes() {
    let mut resized = result(
        SnapshotStatus::Changed,
        FileStatus::Changed,
        FileStatus::Matched,
    );
    resized.png.as_mut().unwrap().golden_size = Some([200, 80]);
    let detail = one(&resized).detail.unwrap();
    assert!(detail.contains("160x80 and its golden 200x80"), "{detail}");
    assert!(!detail.contains("draw text"), "{detail}");
}

#[test]
fn a_snapshot_that_could_not_be_taken_fails_with_its_error() {
    let mut failed = result(
        SnapshotStatus::Failed,
        FileStatus::Missing,
        FileStatus::Missing,
    );
    failed.error = Some(String::from("panicked while capturing the snapshot"));
    let check = one(&failed);
    assert_eq!(check.status, CheckStatus::Failed);
    assert_eq!(
        check.detail.as_deref(),
        Some("panicked while capturing the snapshot")
    );
}

#[test]
fn a_canvas_kept_at_both_moments_fails_when_either_does_and_names_each() {
    let mut start = result(
        SnapshotStatus::Matched,
        FileStatus::Matched,
        FileStatus::Matched,
    );
    start.id = String::from("demo--card--a--start");
    start.moment = Moment::Start;
    let end = result(
        SnapshotStatus::Changed,
        FileStatus::Matched,
        FileStatus::Changed,
    );
    let check = describe([&start, &end]);
    assert_eq!(check.status, CheckStatus::Failed);
    let detail = check.detail.unwrap();
    assert!(detail.starts_with("start: matched; end: "), "{detail}");
    assert!(detail.contains("the draw text changed"), "{detail}");
}
