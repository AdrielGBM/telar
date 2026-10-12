//! Testing previews, as `cargo telar test` does in the app it builds: each preview, and each cell of its matrix, built and laid out on a canvas of its own, played, checked for accessibility and compared against its snapshots, with a line printed for each and `report.json` written for the run.
//!
//! What a run found, and the options it reads, are here on every target, so a review panel reads `report.json` wherever it runs; the run itself, [`run_previews`], only where previews lay text out as the application's window does.

mod options;
mod report;
#[cfg(not(target_os = "android"))]
mod run;
mod snapshot_check;

#[cfg(all(feature = "preview-snapshots", not(target_os = "android")))]
#[path = "snapshots.rs"]
mod snapshots;
#[cfg(all(not(feature = "preview-snapshots"), not(target_os = "android")))]
#[path = "no_snapshots.rs"]
mod snapshots;

use std::path::Path;

pub use options::{A11yLevel, OptionsError, PreviewFilter, SnapshotSetting, TestOptions};
pub use report::{
    A11yCheck, A11yFinding, BuildCheck, CellReport, CheckStatus, PlayCheck, PlayStepReport,
    PreviewReport, SnapshotCheck, TestReport, TestRun, TestSummary,
};
#[cfg(not(target_os = "android"))]
pub use run::{__test_library, run_previews};

/// `path` relative to the directory the run was started from, where it is inside it: what a person reads in a terminal and a CI log.
fn shown(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}
