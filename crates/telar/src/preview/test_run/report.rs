//! What a test run found, preview by preview and cell by cell, as `report.json` carries it beside the snapshot report.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::options::{A11yLevel, SnapshotSetting};
use super::snapshot_check::describe;
use crate::preview::a11y;
use crate::preview::snapshot::{SnapshotError, SnapshotReport};

/// A whole test run: what it was asked to do, each preview and cell it checked, and what failed outside any of them.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TestRun {
    /// The glob the run was narrowed to, if any.
    pub filter: Option<String>,
    pub a11y: A11yLevel,
    pub snapshots: SnapshotSetting,
    pub summary: TestSummary,
    /// What failed the run without belonging to one preview: two previews under one id, snapshots that could not be set up, a filter that matched nothing.
    pub errors: Vec<String>,
    pub previews: Vec<PreviewReport>,
}

impl TestRun {
    pub fn passed(&self) -> bool {
        self.summary.failed == 0
    }

    pub(super) fn summarize(&mut self) {
        let cells = self.previews.iter().flat_map(|preview| &preview.cells);
        let (mut passed, mut failed, mut warned) = (0, 0, 0);
        for cell in cells {
            match cell.passed {
                true => passed += 1,
                false => failed += 1,
            }
            if cell.a11y.status == CheckStatus::Warned {
                warned += 1;
            }
        }
        self.summary = TestSummary {
            previews: self.previews.len(),
            checked: passed + failed,
            passed,
            failed: failed + self.errors.len(),
            warned,
        };
    }
}

/// How the run came out, counted.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestSummary {
    pub previews: usize,
    /// Previews and matrix cells checked, each counted once.
    pub checked: usize,
    pub passed: usize,
    /// Previews and cells that failed, and each of [`TestRun::errors`].
    pub failed: usize,
    /// Previews and cells that passed or failed with accessibility warnings.
    pub warned: usize,
}

/// One preview: where it is written and how it and each cell of its matrix came out, the preview itself first.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PreviewReport {
    pub id: String,
    pub title: String,
    pub component: String,
    pub name: String,
    /// The file the preview is written in, empty when unknown.
    pub file: String,
    /// Its 1-based line there, 0 when unknown.
    pub line: u32,
    pub cells: Vec<CellReport>,
}

/// The preview as it opens, or one cell of its matrix, through each check in the order they ran.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellReport {
    /// The preview's id, or the cell's: `<preview-id>--<axis>=<value>…`.
    pub id: String,
    /// The cell's axis and value pairs, in axis order; empty for the preview itself.
    pub cell: Vec<(String, String)>,
    pub passed: bool,
    pub build: BuildCheck,
    pub play: PlayCheck,
    pub a11y: A11yCheck,
    pub snapshot: SnapshotCheck,
}

impl CellReport {
    pub(super) fn new(id: String, cell: Vec<(String, String)>) -> Self {
        Self {
            id,
            cell,
            ..Self::default()
        }
    }

    pub(super) fn settle(mut self) -> Self {
        self.passed = [
            self.build.status,
            self.play.status,
            self.a11y.status,
            self.snapshot.status,
        ]
        .iter()
        .all(|status| *status != CheckStatus::Failed);
        self
    }

    /// What a run prints under the cell's line: each check's error and each accessibility finding, one per entry, prefixed with the check it came from.
    pub fn details(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(error) = &self.build.error {
            lines.push(format!("build: {error}"));
        }
        if let Some(error) = &self.play.error {
            lines.push(format!("play: {error}"));
        }
        if let Some(error) = &self.a11y.error {
            lines.push(format!("a11y: {error}"));
        }
        lines.extend(
            self.a11y
                .violations
                .iter()
                .map(|finding| format!("a11y: {}", finding.line())),
        );
        if self.snapshot.status == CheckStatus::Failed
            && let Some(detail) = &self.snapshot.detail
        {
            lines.push(format!("snapshot: {detail}"));
        }
        lines
    }
}

/// How one check came out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Passed,
    /// Passed with findings worth reading: accessibility warnings, or errors under `--a11y warn`.
    Warned,
    Failed,
    /// Not run: turned off, nothing to run, or an earlier check failed.
    #[default]
    Skipped,
}

/// Building the preview and laying it out on its canvas.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildCheck {
    pub status: CheckStatus,
    /// How many draw commands the canvas opened on.
    pub draw_commands: usize,
    pub error: Option<String>,
}

/// The preview's play function.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayCheck {
    pub status: CheckStatus,
    pub steps: Vec<PlayStepReport>,
    /// Why the play stopped.
    pub error: Option<String>,
}

/// One step of a play.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayStepReport {
    /// What was done, as a panel lists it: `click button "Save"`.
    pub action: String,
    pub error: Option<String>,
}

/// The accessibility checks, on the canvas as the play left it.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct A11yCheck {
    pub status: CheckStatus,
    pub violations: Vec<A11yFinding>,
    /// Why the checks could not run.
    pub error: Option<String>,
}

/// One accessibility finding.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct A11yFinding {
    /// The rule's id, such as `contrast`.
    pub rule: String,
    /// `error` or `warning`.
    pub severity: String,
    pub message: String,
    /// Where, as `[x, y, width, height]` in the canvas's logical px.
    pub rect: Option<[f32; 4]>,
}

impl A11yFinding {
    pub(super) fn of(violation: &a11y::Violation) -> Self {
        Self {
            rule: violation.rule.id().to_string(),
            severity: violation.severity.as_str().to_string(),
            message: violation.message.clone(),
            rect: violation
                .rect
                .map(|rect| [rect.x, rect.y, rect.width, rect.height]),
        }
    }

    /// The finding as a run prints it.
    pub fn line(&self) -> String {
        format!("{:<7}  {:<16}  {}", self.severity, self.rule, self.message)
    }
}

/// The preview's snapshot, taken as its canvas opened. The full result, with the paths of its goldens and diffs, is in the snapshot report beside this one.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotCheck {
    pub status: CheckStatus,
    /// What the comparison found, in words.
    pub detail: Option<String>,
}

/// `report.json`: the snapshot report, which a review panel reads as it is, with the test run beside it under `test`.
///
/// ```json
/// {
///   "version": 1, "mode": "compare", "goldens": "…", "out": "…", "tolerance": { … }, "summary": { … }, "snapshots": [ … ],
///   "test": {
///     "filter": null, "a11y": "warn", "snapshots": "compare",
///     "summary": { "previews": 1, "checked": 2, "passed": 1, "failed": 1, "warned": 0 },
///     "errors": [],
///     "previews": [{
///       "id": "catalogue--button--primary", "title": "Button", "component": "button", "name": "Primary", "file": "…/button.rsx", "line": 12,
///       "cells": [{
///         "id": "catalogue--button--primary--mode=dark", "cell": [["mode", "dark"]], "passed": false,
///         "build": { "status": "passed", "draw_commands": 14, "error": null },
///         "play": { "status": "failed", "steps": [{ "action": "click button \"Save\"", "error": "found no button \"Save\"" }], "error": "found no button \"Save\"" },
///         "a11y": { "status": "passed", "violations": [], "error": null },
///         "snapshot": { "status": "passed", "detail": "matched" }
///       }]
///     }]
///   }
/// }
/// ```
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TestReport {
    #[serde(flatten)]
    pub snapshots: SnapshotReport,
    pub test: TestRun,
}

impl TestReport {
    pub fn new(snapshots: SnapshotReport, test: TestRun) -> Self {
        Self { snapshots, test }
    }

    pub fn read(path: &Path) -> io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(io::Error::other)
    }

    pub fn write(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        std::fs::write(path, json)
    }

    /// [`SnapshotReport::accept`], with the cell the snapshot `id` is of reported as a run that wrote its goldens reports it, and the run counted again.
    pub fn accept(&mut self, id: &str) -> Result<(), SnapshotError> {
        let check = describe(self.snapshots.accept(id)?);
        let cells = self
            .test
            .previews
            .iter_mut()
            .flat_map(|preview| preview.cells.iter_mut())
            .filter(|cell| cell.id == id);
        for cell in cells {
            cell.snapshot = check.clone();
            *cell = std::mem::take(cell).settle();
        }
        self.test.summarize();
        Ok(())
    }
}

#[cfg(test)]
#[path = "report_test.rs"]
mod tests;
