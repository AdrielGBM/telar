//! What the workspace's last `cargo telar test` found of each cell's snapshot, read from the `report.json` it wrote.

use std::collections::HashMap;

use serde::Deserialize;
use telar::preview::test_run::{CheckStatus, TestRun};

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Snapshots(HashMap<String, CheckStatus>);

#[derive(Deserialize)]
struct Written {
    test: TestRun,
}

impl Snapshots {
    /// Nothing where there is no report, or one this workshop cannot read.
    pub(crate) fn read() -> Self {
        telar::preview::host::test_report_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map_or_else(Self::default, |text| Self::parse(&text))
    }

    pub(crate) fn parse(text: &str) -> Self {
        let Ok(Written { test }) = serde_json::from_str(text) else {
            return Self::default();
        };
        let checked = test
            .previews
            .into_iter()
            .flat_map(|preview| preview.cells)
            .filter(|cell| cell.snapshot.status != CheckStatus::Skipped)
            .map(|cell| (cell.id, cell.snapshot.status))
            .collect();
        Self(checked)
    }

    /// `None` for a cell the run took no snapshot of.
    pub(crate) fn status(&self, id: &str) -> Option<CheckStatus> {
        self.0.get(id).copied()
    }
}
