//! What the workspace's last `cargo telar test` found of each snapshot, read from the `report.json` it wrote, and accepting a drift it found.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::{Deserialize, Serialize};
use telar::preview::snapshot::{Moment, SnapshotReport, SnapshotResult, SnapshotStatus};
use telar::preview::test_run::TestReport;
use telar::{RwSignal, batch, hot_signal, signal};

use crate::state::keys;

/// How the Snapshot panel sets the picture captured against its golden.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum CompareMode {
    /// The golden, the picture captured and where they differ, beside each other.
    #[default]
    SideBySide,
    /// The picture captured over its golden, as see-through as a slider says.
    Overlay,
    /// The golden up to a line a slider moves, the picture captured past it.
    Swipe,
}

/// The report as the workshop last read it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Loaded {
    /// Nowhere to read one from, or no test run has written one yet.
    Absent,
    /// Why the report there cannot be read.
    Unreadable(String),
    Read(Rc<TestReport>),
}

/// The last test run's snapshots, the way the Snapshot panel compares them, and accepting one.
///
/// A cheap handle: clones share one state. The report is read as the workshop's tree is built, so a hot reload reads it again, and on [`reload`](Self::reload).
#[derive(Clone)]
pub(crate) struct SnapshotReview {
    path: Option<Rc<Path>>,
    loaded: RwSignal<Loaded>,
    failure: RwSignal<Option<String>>,
    mode: RwSignal<CompareMode>,
    opacity: RwSignal<f32>,
    swipe: RwSignal<f32>,
    moment: RwSignal<Moment>,
}

impl SnapshotReview {
    /// The review of the report at `path`, read now; `None` for a workshop that does not know where its workspace's report is.
    pub(crate) fn reading(path: Option<PathBuf>) -> Self {
        let path: Option<Rc<Path>> = path.map(Rc::from);
        Self {
            loaded: signal(load(path.as_deref())),
            path,
            failure: signal(None),
            mode: hot_signal(keys::SNAPSHOT_MODE, CompareMode::default()),
            opacity: signal(0.5),
            swipe: signal(0.5),
            moment: signal(Moment::End),
        }
    }

    /// Reads the report again, as a test run run since left it.
    pub(crate) fn reload(&self) {
        let loaded = load(self.path.as_deref());
        batch(|| {
            self.failure.set_if_changed(None);
            self.loaded.set_if_changed(loaded);
        });
    }

    /// Tracked.
    pub(crate) fn loaded(&self) -> Loaded {
        self.loaded.get()
    }

    /// What the run found of each snapshot of `subject`, a preview's or a matrix cell's canvas, in the order they were taken: one, or the start and the end of its play. Tracked.
    pub(crate) fn results(&self, subject: &str) -> Vec<SnapshotResult> {
        self.loaded.with(|loaded| match loaded {
            Loaded::Read(report) => report
                .snapshots
                .snapshots
                .iter()
                .filter(|result| result.subject == subject)
                .cloned()
                .collect(),
            Loaded::Absent | Loaded::Unreadable(_) => Vec::new(),
        })
    }

    /// How the snapshots of `subject` stand: the first that failed the run, else the first taken. Tracked.
    pub(crate) fn status(&self, subject: &str) -> Option<SnapshotStatus> {
        let statuses: Vec<SnapshotStatus> = self
            .results(subject)
            .iter()
            .map(|result| result.status)
            .collect();
        statuses
            .iter()
            .find(|status| status.is_failure())
            .or(statuses.first())
            .copied()
    }

    /// How many snapshots of the preview `id`, of its canvas and of each cell of its matrix, failed the run: drifted, missing a golden or not taken. Tracked.
    pub(crate) fn drifted(&self, preview: &str) -> usize {
        self.loaded.with(|loaded| match loaded {
            Loaded::Read(report) => report
                .snapshots
                .snapshots
                .iter()
                .filter(|result| result.preview == preview && result.status.is_failure())
                .count(),
            Loaded::Absent | Loaded::Unreadable(_) => 0,
        })
    }

    /// Makes what the run captured for `id` its goldens, through the path `cargo telar test --update-snapshots` writes them by, and records it in the report on disk. The report is read again first, so a run that finished since is what is changed.
    pub(crate) fn accept(&self, id: &str) {
        let Some(path) = self.path.as_deref() else {
            return;
        };
        let accepted = TestReport::read(path)
            .map_err(|error| error.to_string())
            .and_then(|mut report| {
                report.accept(id).map_err(|error| error.to_string())?;
                report.write(path).map_err(|error| error.to_string())?;
                Ok(report)
            });
        batch(|| match accepted {
            Ok(report) => {
                self.failure.set_if_changed(None);
                self.loaded.set(Loaded::Read(Rc::new(report)));
            }
            Err(error) => self.failure.set(Some(error)),
        });
    }

    /// Why the last accept failed, until the next one or a reload. Tracked.
    pub(crate) fn failure(&self) -> Option<String> {
        self.failure.get()
    }

    pub(crate) fn mode(&self) -> RwSignal<CompareMode> {
        self.mode
    }

    /// How opaque the picture captured is over its golden in [`CompareMode::Overlay`], 0–1.
    pub(crate) fn opacity(&self) -> RwSignal<f32> {
        self.opacity
    }

    /// Which moment of its play the panel reviews of a canvas snapshotted at both.
    pub(crate) fn moment(&self) -> RwSignal<Moment> {
        self.moment
    }

    /// Where the line between the golden and the picture captured sits across the picture in [`CompareMode::Swipe`], 0–1.
    pub(crate) fn swipe(&self) -> RwSignal<f32> {
        self.swipe
    }
}

fn load(path: Option<&Path>) -> Loaded {
    let Some(path) = path else {
        return Loaded::Absent;
    };
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Loaded::Absent,
        Err(error) => Loaded::Unreadable(error.to_string()),
    }
}

/// The report `text` holds, refused when a version of telar this workshop was not built with wrote it.
pub(crate) fn parse(text: &str) -> Loaded {
    match serde_json::from_str::<TestReport>(text) {
        Ok(report) if report.snapshots.version == SnapshotReport::VERSION => {
            Loaded::Read(Rc::new(report))
        }
        Ok(report) => Loaded::Unreadable(format!(
            "it is version {} and this workshop reads version {}",
            report.snapshots.version,
            SnapshotReport::VERSION
        )),
        Err(error) => Loaded::Unreadable(error.to_string()),
    }
}
