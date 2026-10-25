use std::path::{Path, PathBuf};

use telar::preview::host::PreviewRequest;
use telar::preview::snapshot::{SnapshotReport, SnapshotStatus};
use telar::preview::test_run::{CellReport, CheckStatus, PreviewReport, TestReport, TestRun};
use telar::preview::{PreviewCtx, PreviewEntry};
use telar::testing::{Scoped, centre, mount, press, release, route, texts};
use telar::{
    AccessNode, App, AppRuntime, Color, ComponentList, DrawCommand, Event, LayoutError, LayoutItem,
    LayoutStyle, LocalApp, Rect, Role, Text, TextStyle, box_item, for_each_with_matrix,
    transform_clip_rect,
};
use telar_components::TokenKind;
use tiny_skia::Pixmap;

use super::panel::diff_spans;
use super::review::{Loaded, parse};
use super::*;
use crate::WorkshopApp;
use crate::state::{ViewMode, WorkshopState};
use crate::test_support::{control, settle};

const WIDTH: u32 = 1600;
const HEIGHT: u32 = 1400;
const DRIFTED: &str = "review--badge--drifted";
const MATCHED: &str = "review--badge--matched";
const UNTESTED: &str = "review--badge--untested";
const PLAYED: &str = "review--badge--played";
const PLAYED_START: &str = "review--badge--played--start";
const CAPTURED_DRAW: &str = "frame 8x8\nrect 0,0 8x8 fill=#2255aaff\n";
const GOLDEN_DRAW: &str = "frame 8x8\nrect 0,0 8x8 fill=#aa5522ff\n";

fn plain(_: &PreviewCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Text::new(
        || String::from("plain body"),
        LayoutStyle::new(),
        || TextStyle::new(13.0, Color::BLACK),
    )?))
}

fn entries() -> Vec<PreviewEntry> {
    vec![
        PreviewEntry::new(DRIFTED, "badge", "Primary", plain).title("Review/Badge"),
        PreviewEntry::new(MATCHED, "badge", "Secondary", plain).title("Review/Badge"),
        PreviewEntry::new(UNTESTED, "badge", "Tertiary", plain).title("Review/Badge"),
        PreviewEntry::new(PLAYED, "badge", "Played", plain).title("Review/Badge"),
    ]
}

fn png(color: [u8; 4]) -> Vec<u8> {
    let mut pixmap = Pixmap::new(8, 8).unwrap();
    pixmap.fill(tiny_skia::Color::from_rgba8(
        color[0], color[1], color[2], color[3],
    ));
    pixmap.encode_png().unwrap()
}

/// What `cargo telar test` leaves behind: goldens beside a package, and under the target directory what did not match them with the report of the run.
struct Run {
    root: PathBuf,
}

impl Run {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "telar_workshop_snapshot_{name}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        Self { root }
    }

    fn goldens(&self) -> PathBuf {
        self.root.join("package/previews/__snapshots__")
    }

    fn out(&self) -> PathBuf {
        self.root.join("target/telar-snapshots")
    }

    fn report(&self) -> PathBuf {
        self.out().join(SnapshotReport::FILE_NAME)
    }

    fn golden(&self, id: &str, suffix: &str) -> PathBuf {
        self.goldens().join(format!("{id}{suffix}"))
    }

    fn found(&self, id: &str, suffix: &str) -> PathBuf {
        self.out().join(format!("{id}{suffix}"))
    }

    /// A run in which `DRIFTED` changed colour and `MATCHED` still matched.
    fn write(&self) {
        std::fs::create_dir_all(self.goldens()).unwrap();
        std::fs::create_dir_all(self.out()).unwrap();
        for id in [DRIFTED, MATCHED] {
            std::fs::write(self.golden(id, ".png"), png([0xaa, 0x55, 0x22, 0xff])).unwrap();
            std::fs::write(self.golden(id, ".draw.txt"), GOLDEN_DRAW).unwrap();
        }
        std::fs::write(self.found(DRIFTED, ".png"), png([0x22, 0x55, 0xaa, 0xff])).unwrap();
        std::fs::write(self.found(DRIFTED, ".draw.txt"), CAPTURED_DRAW).unwrap();
        std::fs::write(
            self.found(DRIFTED, ".diff.png"),
            png([0xff, 0x00, 0x50, 0xff]),
        )
        .unwrap();
        std::fs::write(
            self.found(DRIFTED, ".draw.diff"),
            "--- golden\n+++ captured\n@@ -2 +2 @@\n-rect 0,0 8x8 fill=#aa5522ff\n+rect 0,0 8x8 fill=#2255aaff\n",
        )
        .unwrap();
        std::fs::write(self.report(), self.report_json()).unwrap();
    }

    fn report_json(&self) -> String {
        self.report_json_with(Vec::new())
    }

    /// The report of the run [`write`](Self::write) leaves, with `more` snapshots of other previews after its own.
    fn report_json_with(&self, more: Vec<serde_json::Value>) -> String {
        let path = |path: PathBuf| path.display().to_string();
        let drifted = serde_json::json!({
            "id": DRIFTED, "preview": DRIFTED, "subject": DRIFTED, "cell": [], "moment": "start",
            "status": "changed",
            "png": {
                "status": "changed",
                "golden": path(self.golden(DRIFTED, ".png")),
                "actual": path(self.found(DRIFTED, ".png")),
                "diff": path(self.found(DRIFTED, ".diff.png")),
                "size": [8, 8], "golden_size": [8, 8],
                "differing_pixels": 64, "max_channel_delta": 136
            },
            "draw": {
                "status": "changed",
                "golden": path(self.golden(DRIFTED, ".draw.txt")),
                "actual": path(self.found(DRIFTED, ".draw.txt")),
                "diff": path(self.found(DRIFTED, ".draw.diff"))
            },
            "error": null
        });
        let matched = serde_json::json!({
            "id": MATCHED, "preview": MATCHED, "subject": MATCHED, "cell": [], "moment": "start",
            "status": "matched",
            "png": {
                "status": "matched",
                "golden": path(self.golden(MATCHED, ".png")),
                "actual": null, "diff": null,
                "size": [8, 8], "golden_size": [8, 8],
                "differing_pixels": 0, "max_channel_delta": 0
            },
            "draw": {
                "status": "matched",
                "golden": path(self.golden(MATCHED, ".draw.txt")),
                "actual": null, "diff": null
            },
            "error": null
        });
        let mut run = TestRun::default();
        for (id, status) in [
            (DRIFTED, CheckStatus::Failed),
            (MATCHED, CheckStatus::Passed),
        ] {
            let mut cell = CellReport::default();
            cell.id = id.to_string();
            cell.passed = status == CheckStatus::Passed;
            cell.snapshot.status = status;
            let mut preview = PreviewReport::default();
            preview.id = id.to_string();
            preview.cells.push(cell);
            run.previews.push(preview);
        }
        run.summary.checked = 2;
        run.summary.passed = 1;
        run.summary.failed = 1;
        let snapshots = [vec![drifted, matched], more].concat();
        serde_json::json!({
            "version": SnapshotReport::VERSION,
            "mode": "compare",
            "goldens": path(self.goldens()),
            "out": path(self.out()),
            "tolerance": { "channel": 2, "max_pixels": 0 },
            "summary": { "total": 2, "matched": 1, "updated": 0, "failed": 1 },
            "snapshots": snapshots,
            "test": run,
        })
        .to_string()
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Workshop {
    runtime: LocalApp<WorkshopApp>,
    tree: Scoped<ComponentList>,
}

impl Workshop {
    fn open(id: &str, report: &Path) -> Self {
        telar::install_default_text_metrics();
        telar::hot_restore_json(r#"{"@workshop/panel.size":"700.0"}"#);
        let app = WorkshopApp::new(entries()).select(id).test_report(report);
        let runtime = LocalApp(app);
        let tree = mount(|| runtime.0.root(), WIDTH, HEIGHT);
        let mut workshop = Self { runtime, tree };
        settle(&workshop.tree);
        workshop.click(Role::Tab, "Snapshot");
        workshop
    }

    fn texts(&self) -> Vec<String> {
        texts(&self.tree)
    }

    fn pictures(&self) -> usize {
        self.drawn_pictures().len()
    }

    /// Each picture drawn, where it lands, and the clip it is drawn under.
    fn drawn_pictures(&self) -> Vec<(Rect, Option<Rect>)> {
        let mut clips = Vec::new();
        let mut pictures = Vec::new();
        for_each_with_matrix(&self.tree.commands(), |command, matrix| match command {
            DrawCommand::PushClip { rect, .. } => clips.push(transform_clip_rect(matrix, *rect)),
            DrawCommand::PopClip => {
                clips.pop();
            }
            DrawCommand::Image { rect, .. } => {
                pictures.push((transform_clip_rect(matrix, *rect), clips.last().copied()))
            }
            _ => {}
        });
        pictures
    }

    fn draws(&self, text: &str) -> bool {
        self.texts().iter().any(|drawn| drawn == text)
    }

    fn access(&self) -> Vec<AccessNode> {
        self.runtime.access_snapshot(&self.tree.commands())
    }

    fn has(&self, role: Role, name: &str) -> bool {
        self.access()
            .iter()
            .any(|node| node.id.is_some() && node.role == role && node.name == name)
    }

    fn dispatch(&mut self, event: Event) {
        route(&mut self.tree, &event);
        settle(&self.tree);
    }

    fn click(&mut self, role: Role, name: &str) {
        let (x, y) = centre(control(&self.access(), role, name).rect);
        self.dispatch(press(x, y));
        self.dispatch(release(x, y));
    }
}

#[test]
fn a_drifted_snapshot_is_accepted_from_the_panel_and_its_goldens_change_on_disk() {
    let run = Run::new("accept");
    run.write();
    let mut workshop = Workshop::open(DRIFTED, &run.report());
    for text in [
        DRIFTED,
        "64 pixels differ, by up to 136 a channel · The draw text changed",
        "Baseline",
        "Current",
        "Diff",
        "Draw text diff",
    ] {
        assert!(workshop.draws(text), "{text} in {:?}", workshop.texts());
    }
    assert!(workshop.draws("Drifted"), "the sidebar marks the preview");
    assert!(workshop.draws("1"), "and the tab counts it");

    workshop.click(Role::Button, "Accept");

    assert_eq!(
        std::fs::read(run.golden(DRIFTED, ".png")).unwrap(),
        png([0x22, 0x55, 0xaa, 0xff]),
        "the picture captured is the golden now"
    );
    assert_eq!(
        std::fs::read_to_string(run.golden(DRIFTED, ".draw.txt")).unwrap(),
        CAPTURED_DRAW
    );
    assert!(
        !run.found(DRIFTED, ".png").exists(),
        "and no longer a finding of the run"
    );
    let report = TestReport::read(&run.report()).unwrap();
    assert_eq!(
        report.snapshots.snapshots[0].status,
        SnapshotStatus::Updated
    );
    assert!(report.snapshots.passed() && report.test.passed());
    assert!(
        workshop.draws("The goldens were written from this snapshot"),
        "{:?}",
        workshop.texts()
    );
    assert!(!workshop.has(Role::Button, "Accept"));
    assert!(!workshop.draws("Drifted"), "the sidebar badge goes");
    assert!(!workshop.draws("1"), "and so does the tab's");
}

#[test]
fn the_comparison_switches_between_side_by_side_overlay_and_swipe() {
    let run = Run::new("modes");
    run.write();
    let mut workshop = Workshop::open(DRIFTED, &run.report());
    assert!(!workshop.has(Role::Slider, "Current picture opacity"));
    assert_eq!(
        workshop.pictures(),
        3,
        "the golden, the picture captured and the diff"
    );

    workshop.click(Role::Tab, "Overlay");
    assert!(workshop.has(Role::Slider, "Current picture opacity"));
    assert_eq!(workshop.pictures(), 2);
    assert!(
        !workshop.draws("Diff"),
        "an overlay sets two pictures together"
    );

    workshop.click(Role::Tab, "Swipe");
    assert!(workshop.has(Role::Slider, "Baseline up to here, current past it"));
    let pictures = workshop.drawn_pictures();
    assert_eq!(pictures.len(), 2);
    let (current, baseline) = (pictures[0].0, pictures[1]);
    assert_eq!(baseline.0, current, "both pictures land in the same place");
    let cut = baseline.1.expect("the golden is cut at the line");
    let middle = current.x + current.width / 2.0;
    assert!(
        (cut.x + cut.width - middle).abs() < 1.0,
        "the line starts halfway across the picture: {cut:?} over {current:?}"
    );

    workshop.click(Role::Tab, "Side by side");
    assert!(workshop.draws("Diff"));
}

#[test]
fn a_matched_snapshot_shows_its_golden_and_nothing_to_accept() {
    let run = Run::new("matched");
    run.write();
    let workshop = Workshop::open(MATCHED, &run.report());
    assert!(workshop.draws("Snapshot matches"), "{:?}", workshop.texts());
    assert!(workshop.draws("Baseline"));
    assert!(!workshop.draws("Current"));
    assert!(!workshop.has(Role::Button, "Accept"));
    assert!(!workshop.has(Role::Tab, "Overlay"));
}

#[test]
fn the_panel_says_when_there_is_no_report_yet() {
    let run = Run::new("absent");
    let workshop = Workshop::open(DRIFTED, &run.report());
    assert!(
        workshop.draws("No test report yet"),
        "{:?}",
        workshop.texts()
    );
}

#[test]
fn the_panel_says_when_the_report_has_no_snapshot_of_the_preview() {
    let run = Run::new("untested");
    run.write();
    let workshop = Workshop::open(UNTESTED, &run.report());
    assert!(
        workshop.draws("The last test run has no snapshot of this preview or cell"),
        "{:?}",
        workshop.texts()
    );
}

#[test]
fn an_accept_that_cannot_read_what_was_captured_says_why() {
    let run = Run::new("unreadable");
    run.write();
    let mut workshop = Workshop::open(DRIFTED, &run.report());
    std::fs::remove_file(run.found(DRIFTED, ".draw.txt")).unwrap();

    workshop.click(Role::Button, "Accept");

    assert!(
        workshop
            .texts()
            .iter()
            .any(|text| text.starts_with("The snapshot could not be accepted: ")),
        "{:?}",
        workshop.texts()
    );
    assert_eq!(
        std::fs::read_to_string(run.golden(DRIFTED, ".draw.txt")).unwrap(),
        GOLDEN_DRAW,
        "the goldens are left as they were"
    );
}

#[test]
fn the_report_counts_what_failed_each_preview() {
    let run = Run::new("parse");
    run.write();
    let review = SnapshotReview::reading(Some(run.report()));
    assert_eq!(review.status(DRIFTED), Some(SnapshotStatus::Changed));
    assert_eq!(review.status(MATCHED), Some(SnapshotStatus::Matched));
    assert_eq!(review.status(UNTESTED), None);
    assert_eq!(review.drifted(DRIFTED), 1);
    assert_eq!(review.drifted(MATCHED), 0);

    assert!(matches!(parse("not json"), Loaded::Unreadable(_)));
    let newer = run.report_json().replacen(
        &format!("\"version\":{}", SnapshotReport::VERSION),
        "\"version\":99",
        1,
    );
    assert!(matches!(parse(&newer), Loaded::Unreadable(error) if error.contains("99")));
    assert_eq!(
        SnapshotReview::reading(Some(run.root.join("nowhere.json"))).loaded(),
        Loaded::Absent
    );
    assert_eq!(SnapshotReview::reading(None).loaded(), Loaded::Absent);
}

#[test]
fn the_matrix_view_reviews_the_focused_cell() {
    let state = WorkshopState::new(entries().into(), &PreviewRequest::default());
    state.select(DRIFTED);
    assert_eq!(subject(&state).as_deref(), Some(DRIFTED));

    let cell = format!("{DRIFTED}--mode=dark");
    state.matrix_focus().set(Some(cell.clone()));
    assert_eq!(
        subject(&state).as_deref(),
        Some(DRIFTED),
        "only the Matrix view follows the focused cell"
    );
    state.view().set(ViewMode::Matrix);
    assert_eq!(subject(&state), Some(cell));
    state
        .matrix_focus()
        .set(Some(format!("{MATCHED}--mode=dark")));
    assert_eq!(subject(&state).as_deref(), Some(DRIFTED));
}

#[test]
fn a_draw_diff_marks_what_was_removed_and_added() {
    let diff = "--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n same\n";
    let spans = diff_spans(diff);
    let marked: Vec<(&str, TokenKind)> = spans
        .iter()
        .map(|span| (&diff[span.range.clone()], span.kind))
        .collect();
    assert_eq!(
        marked,
        [
            ("@@ -1 +1 @@", TokenKind::Comment),
            ("-old", TokenKind::Error),
            ("+new", TokenKind::String),
        ]
    );
}

#[test]
fn a_canvas_snapshotted_at_both_ends_of_its_play_reviews_either() {
    let run = Run::new("moments");
    run.write();
    std::fs::write(
        run.golden(PLAYED_START, ".png"),
        png([0xaa, 0x55, 0x22, 0xff]),
    )
    .unwrap();
    std::fs::write(run.golden(PLAYED_START, ".draw.txt"), GOLDEN_DRAW).unwrap();
    let path = |path: PathBuf| path.display().to_string();
    let start = serde_json::json!({
        "id": PLAYED_START, "preview": PLAYED, "subject": PLAYED, "cell": [], "moment": "start",
        "status": "matched",
        "png": {
            "status": "matched",
            "golden": path(run.golden(PLAYED_START, ".png")),
            "actual": null, "diff": null,
            "size": [8, 8], "golden_size": [8, 8],
            "differing_pixels": 0, "max_channel_delta": 0
        },
        "draw": {
            "status": "matched",
            "golden": path(run.golden(PLAYED_START, ".draw.txt")),
            "actual": null, "diff": null
        },
        "error": null
    });
    let end = serde_json::json!({
        "id": PLAYED, "preview": PLAYED, "subject": PLAYED, "cell": [], "moment": "end",
        "status": "failed", "png": null, "draw": null,
        "error": "not taken: the play stopped before its end: found no button \"Open\""
    });
    std::fs::write(run.report(), run.report_json_with(vec![start, end])).unwrap();
    let review = SnapshotReview::reading(Some(run.report()));
    assert_eq!(review.status(PLAYED), Some(SnapshotStatus::Failed));
    assert_eq!(review.drifted(PLAYED), 1);

    let mut workshop = Workshop::open(PLAYED, &run.report());
    assert!(
        workshop.has(Role::Tab, "End of play"),
        "{:?}",
        workshop.access()
    );
    assert!(
        workshop
            .texts()
            .iter()
            .any(|text| text.contains("the play stopped before its end")),
        "the end is reviewed first: {:?}",
        workshop.texts()
    );
    assert_eq!(workshop.pictures(), 0, "an end not taken has no picture");

    workshop.click(Role::Tab, "Start of play");
    assert!(workshop.draws(PLAYED_START), "{:?}", workshop.texts());
    assert!(workshop.draws("Snapshot matches"));
    assert_eq!(workshop.pictures(), 1, "the golden of the start");
}

/// A test run run beside the workshop is shown as it lands, without asking: the review looks at the report every [`super::review::WATCH_EVERY`], and reads it only once it is not the file it was.
#[test]
fn a_report_a_test_run_rewrites_is_read_again_by_itself() {
    use telar::testing::advance_time;
    telar::reset_timers();
    let run = Run::new("watch");
    let review = SnapshotReview::reading(Some(run.report()));
    assert_eq!(review.loaded(), Loaded::Absent, "no run yet");

    run.write();
    assert_eq!(
        review.loaded(),
        Loaded::Absent,
        "nothing is read before it looks"
    );
    advance_time(super::review::WATCH_EVERY);
    assert_eq!(review.status(DRIFTED), Some(SnapshotStatus::Changed));

    std::fs::write(run.report(), "not json").unwrap();
    advance_time(super::review::WATCH_EVERY);
    assert!(matches!(review.loaded(), Loaded::Unreadable(_)));

    std::fs::remove_file(run.report()).unwrap();
    advance_time(super::review::WATCH_EVERY);
    assert_eq!(
        review.loaded(),
        Loaded::Absent,
        "and gone again once removed"
    );
}
