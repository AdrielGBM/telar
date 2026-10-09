//! Snapshots against goldens on disk: a run that matches, a picture that drifts, a padding change read as a line diff, and an update that rewrites both files.
//!
//! A test binary of its own because a snapshot run makes the declared faces the only ones in the process: a binary whose other tests had loaded the platform's fonts would be refused. Every test here declares the same face, so they share the process happily.

use std::path::{Path, PathBuf};

use telar::preview::snapshot::{
    FileStatus, SnapshotError, SnapshotMode, SnapshotPaths, SnapshotReport, SnapshotStatus,
    Snapshots, Tolerance, accept,
};
use telar::preview::{Axis, Matrices, Matrix, PreviewCtx, PreviewEntry};
use telar::{
    AppConfig, Color, Direction, FontAsset, LayoutError, LayoutItem, LayoutStyle, RectStyle,
    StyledContainer, Text, TextStyle, box_item,
};

const FACE: &[u8] = include_bytes!("../../renderer/renderer-text/test-fonts/TelarTest.ttf");
const ID: &str = "snapshots--badge--default";

type Built = Result<Box<dyn LayoutItem>, LayoutError>;

fn config() -> AppConfig {
    AppConfig::default().with_fonts([FontAsset::embedded(FACE).named("Snapshot Face")])
}

fn badge(padding: f32) -> Built {
    let label = Text::new(
        || String::from("Badge"),
        LayoutStyle::new(),
        || TextStyle::new(14.0, Color::WHITE),
    )?;
    Ok(box_item(StyledContainer::new(
        LayoutStyle::new().padding_all(padding),
        |_| RectStyle::filled(Color::from_rgb_u8(0x22, 0x55, 0xaa), 4.0),
        vec![box_item(label)],
    )?))
}

fn padded_8(_: &PreviewCtx) -> Built {
    badge(8.0)
}

fn padded_12(_: &PreviewCtx) -> Built {
    badge(12.0)
}

fn entry(build: fn(&PreviewCtx) -> Built) -> PreviewEntry {
    PreviewEntry::new(ID, "badge", "default", build).viewport(160.0, 80.0)
}

struct Dirs {
    root: PathBuf,
}

impl Dirs {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "telar_preview_snapshot_{name}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        Self { root }
    }

    fn paths(&self) -> SnapshotPaths {
        SnapshotPaths::new(&self.root.join("package"), &self.root.join("target"))
    }
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run(dirs: &Dirs, mode: SnapshotMode, entry: &PreviewEntry) -> SnapshotReport {
    let mut snapshots = Snapshots::new(dirs.paths(), mode, &config()).expect("declared faces");
    snapshots.check_entry(entry, &Matrices::default());
    snapshots.finish().expect("the report is written")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[test]
fn a_snapshot_matches_the_goldens_an_update_wrote() {
    let dirs = Dirs::new("match");
    let paths = dirs.paths();

    let first = run(&dirs, SnapshotMode::Compare, &entry(padded_8));
    assert_eq!(first.snapshots[0].status, SnapshotStatus::Missing);
    assert!(!first.passed(), "a snapshot with no golden fails a run");
    assert!(
        !paths.golden_png(ID).exists(),
        "and a compare run writes no golden"
    );

    let written = run(&dirs, SnapshotMode::Update, &entry(padded_8));
    assert_eq!(written.snapshots[0].status, SnapshotStatus::Updated);
    assert!(paths.golden_png(ID).exists());
    let draw = read(&paths.golden_draw(ID));
    assert!(draw.starts_with("frame 160x80\n"), "{draw}");
    assert!(
        draw.contains("\"Badge\" size=14 family=sans-serif weight=400 color=#ffffffff"),
        "{draw}"
    );

    let again = run(&dirs, SnapshotMode::Compare, &entry(padded_8));
    let result = &again.snapshots[0];
    assert_eq!(result.status, SnapshotStatus::Matched, "{result:?}");
    assert!(again.passed());
    assert!(
        !paths.out.join(format!("{ID}.png")).exists(),
        "a match leaves nothing behind from the run that missed it"
    );
    assert_eq!(
        SnapshotReport::read(&paths.report()).unwrap(),
        again,
        "the report on disk is the one returned"
    );
    let json = read(&paths.report());
    assert!(json.contains("\"status\": \"matched\""), "{json}");
    assert!(json.contains("\"version\": 1"), "{json}");
}

#[test]
fn pixels_drifting_past_the_tolerance_fail_with_a_diff_picture() {
    let dirs = Dirs::new("drift");
    let paths = dirs.paths();
    run(&dirs, SnapshotMode::Update, &entry(padded_8));
    let golden = paths.golden_png(ID);
    let original = std::fs::read(&golden).unwrap();

    let mut pixmap = golden_png::decode(&original);
    pixmap[0] = pixmap[0].checked_sub(1).unwrap_or(1);
    std::fs::write(&golden, golden_png::encode(&pixmap, &original)).unwrap();
    let nudged = run(&dirs, SnapshotMode::Compare, &entry(padded_8));
    assert_eq!(
        nudged.snapshots[0].status,
        SnapshotStatus::Matched,
        "one step on one channel is within the tolerance"
    );

    let mut pixmap = golden_png::decode(&original);
    for pixel in pixmap.chunks_mut(4).take(40) {
        pixel[0] ^= 0x80;
    }
    std::fs::write(&golden, golden_png::encode(&pixmap, &original)).unwrap();
    let drifted = run(&dirs, SnapshotMode::Compare, &entry(padded_8));
    let result = &drifted.snapshots[0];
    assert_eq!(result.status, SnapshotStatus::Changed);
    let png = result.png.as_ref().unwrap();
    assert_eq!(png.status, FileStatus::Changed);
    assert!(
        png.differing_pixels > 0 && png.differing_pixels <= 40,
        "{png:?}"
    );
    assert!(
        png.diff.as_ref().is_some_and(|diff| diff.exists()),
        "{png:?}"
    );
    assert!(png.actual.as_ref().is_some_and(|actual| actual.exists()));
    assert_eq!(
        result.draw.as_ref().unwrap().status,
        FileStatus::Matched,
        "the draw text did not change, only the picture"
    );

    let mut loose = Snapshots::new(paths.clone(), SnapshotMode::Compare, &config())
        .unwrap()
        .with_tolerance(Tolerance::new(2, 40));
    loose.check_entry(&entry(padded_8), &Matrices::default());
    assert_eq!(
        loose.results()[0].status,
        SnapshotStatus::Matched,
        "a cap of forty differing pixels allows them"
    );
}

#[test]
fn a_padding_change_reads_as_a_line_diff_of_the_draw_text() {
    let dirs = Dirs::new("padding");
    run(&dirs, SnapshotMode::Update, &entry(padded_8));

    let report = run(&dirs, SnapshotMode::Compare, &entry(padded_12));
    let result = &report.snapshots[0];
    assert_eq!(result.status, SnapshotStatus::Changed);
    let draw = result.draw.as_ref().unwrap();
    assert_eq!(draw.status, FileStatus::Changed);
    let diff = read(draw.diff.as_ref().expect("a line diff is written"));
    let removed: Vec<&str> = diff
        .lines()
        .filter(|line| line.starts_with('-') && !line.starts_with("---"))
        .collect();
    let added: Vec<&str> = diff
        .lines()
        .filter(|line| line.starts_with('+') && !line.starts_with("+++"))
        .collect();
    assert!(
        removed
            .iter()
            .any(|line| line.ends_with(" rect 16,16 59x33 fill=#2255aaff radius=4"))
            && removed
                .iter()
                .any(|line| line.ends_with("matrix 1,0,0,1,24,24")),
        "the badge was drawn around its label with an 8 px padding:\n{diff}"
    );
    assert!(
        added
            .iter()
            .any(|line| line.ends_with(" rect 16,16 67x41 fill=#2255aaff radius=4"))
            && added
                .iter()
                .any(|line| line.ends_with("matrix 1,0,0,1,28,28")),
        "and now with a 12 px one:\n{diff}"
    );
    assert!(
        removed.len() == 2 && added.len() == 2,
        "only the lines that moved:\n{diff}"
    );
}

#[test]
fn an_update_rewrites_both_goldens_and_accepting_copies_what_was_captured() {
    let dirs = Dirs::new("update");
    let paths = dirs.paths();
    run(&dirs, SnapshotMode::Update, &entry(padded_8));
    let before = (
        std::fs::read(paths.golden_png(ID)).unwrap(),
        read(&paths.golden_draw(ID)),
    );

    let unchanged = run(&dirs, SnapshotMode::Update, &entry(padded_8));
    assert_eq!(
        unchanged.snapshots[0].status,
        SnapshotStatus::Matched,
        "a golden that still matches is left as it is"
    );

    let updated = run(&dirs, SnapshotMode::Update, &entry(padded_12));
    let result = &updated.snapshots[0];
    assert_eq!(result.status, SnapshotStatus::Updated);
    assert_eq!(result.png.as_ref().unwrap().status, FileStatus::Written);
    assert_eq!(result.draw.as_ref().unwrap().status, FileStatus::Written);
    assert_ne!(std::fs::read(paths.golden_png(ID)).unwrap(), before.0);
    assert_ne!(read(&paths.golden_draw(ID)), before.1);
    assert_eq!(
        run(&dirs, SnapshotMode::Compare, &entry(padded_12)).snapshots[0].status,
        SnapshotStatus::Matched
    );

    let drifted = run(&dirs, SnapshotMode::Compare, &entry(padded_8));
    accept(&drifted.snapshots[0]).expect("a changed snapshot can be accepted");
    assert_eq!(read(&paths.golden_draw(ID)), before.1);
    assert_eq!(
        run(&dirs, SnapshotMode::Compare, &entry(padded_8)).snapshots[0].status,
        SnapshotStatus::Matched
    );
}

#[test]
fn each_matrix_cell_is_a_snapshot_of_its_own() {
    let dirs = Dirs::new("matrix");
    let paths = dirs.paths();
    let matrix = entry(padded_8).matrix(Matrix::Axes(&[Axis::Dir(&[
        Direction::Ltr,
        Direction::Rtl,
    ])]));
    let report = run(&dirs, SnapshotMode::Update, &matrix);
    let ids: Vec<&str> = report.snapshots.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            ID.to_string(),
            format!("{ID}--dir=ltr"),
            format!("{ID}--dir=rtl")
        ]
    );
    let rtl = &report.snapshots[2];
    assert_eq!(rtl.preview, ID);
    assert_eq!(rtl.cell, [("dir".to_string(), "rtl".to_string())]);
    for id in &ids {
        assert!(paths.golden_png(id).exists(), "{id}");
        assert!(paths.golden_draw(id).exists(), "{id}");
    }
    assert!(
        read(&paths.golden_draw(ids[2])).starts_with("frame 160x80 dir=rtl"),
        "the cell draws right to left"
    );
}

#[test]
fn a_run_with_no_declared_face_is_refused() {
    let dirs = Dirs::new("no_fonts");
    let refused = Snapshots::new(dirs.paths(), SnapshotMode::Compare, &AppConfig::default());
    assert!(matches!(refused, Err(SnapshotError::Fonts(_))));
}

/// A golden picture's bytes, as premultiplied RGBA, and back: every pixel a preview page draws is opaque, so they are its straight colours too.
mod golden_png {
    use tiny_skia::{IntSize, Pixmap};

    pub fn decode(png: &[u8]) -> Vec<u8> {
        Pixmap::decode_png(png).unwrap().data().to_vec()
    }

    pub fn encode(rgba: &[u8], like: &[u8]) -> Vec<u8> {
        let golden = Pixmap::decode_png(like).unwrap();
        let size = IntSize::from_wh(golden.width(), golden.height()).unwrap();
        Pixmap::from_vec(rgba.to_vec(), size)
            .unwrap()
            .encode_png()
            .unwrap()
    }
}
