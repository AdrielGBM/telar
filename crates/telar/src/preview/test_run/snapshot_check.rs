//! A snapshot's result as a test run reports it, beside the full result in the snapshot report.

use crate::preview::snapshot::{FileStatus, Moment, SnapshotResult, SnapshotStatus};

use super::report::{CheckStatus, SnapshotCheck};
use super::shown;

/// The snapshots of one canvas as a test run reports them: passed when every one did, and what was found, in words, each after the moment it is of when there is more than one.
pub(super) fn describe<'a>(results: impl IntoIterator<Item = &'a SnapshotResult>) -> SnapshotCheck {
    let checks: Vec<(Moment, SnapshotCheck)> = results
        .into_iter()
        .map(|result| (result.moment, describe_one(result)))
        .collect();
    let status = match checks
        .iter()
        .any(|(_, check)| check.status == CheckStatus::Failed)
    {
        true => CheckStatus::Failed,
        false => CheckStatus::Passed,
    };
    let detail = match checks.as_slice() {
        [] => None,
        [(_, check)] => check.detail.clone(),
        several => Some(
            several
                .iter()
                .filter_map(|(moment, check)| {
                    check
                        .detail
                        .as_ref()
                        .map(|detail| format!("{}: {detail}", moment.as_str()))
                })
                .collect::<Vec<_>>()
                .join("; "),
        ),
    };
    SnapshotCheck { status, detail }
}

fn describe_one(result: &SnapshotResult) -> SnapshotCheck {
    let (status, detail) = match result.status {
        SnapshotStatus::Matched => (CheckStatus::Passed, String::from("matched")),
        SnapshotStatus::Updated => (CheckStatus::Passed, String::from("written")),
        SnapshotStatus::Missing => (CheckStatus::Failed, missing(result)),
        SnapshotStatus::Changed => (CheckStatus::Failed, changed(result)),
        SnapshotStatus::Failed => (
            CheckStatus::Failed,
            result
                .error
                .clone()
                .unwrap_or_else(|| String::from("the snapshot could not be taken")),
        ),
    };
    SnapshotCheck {
        status,
        detail: Some(detail),
    }
}

fn missing(result: &SnapshotResult) -> String {
    let png = result.png.as_ref().map(|png| (png.status, &png.golden));
    let draw = result.draw.as_ref().map(|draw| (draw.status, &draw.golden));
    let goldens: Vec<String> = [png, draw]
        .into_iter()
        .flatten()
        .filter(|(status, _)| *status == FileStatus::Missing)
        .map(|(_, golden)| shown(golden))
        .collect();
    format!(
        "no golden yet: {}; write it with `cargo telar test --update-snapshots`",
        goldens.join(", ")
    )
}

fn changed(result: &SnapshotResult) -> String {
    let mut parts = Vec::new();
    if let Some(png) = result
        .png
        .as_ref()
        .filter(|png| png.status == FileStatus::Changed)
    {
        let mut part = match png.golden_size {
            Some(golden) if golden != png.size => format!(
                "the picture is {}x{} and its golden {}x{}",
                png.size[0], png.size[1], golden[0], golden[1]
            ),
            _ => format!(
                "{} pixel(s) moved past the tolerance, by up to {} a channel",
                png.differing_pixels, png.max_channel_delta
            ),
        };
        if let Some(diff) = &png.diff {
            part.push_str(&format!(" (diff: {})", shown(diff)));
        }
        parts.push(part);
    }
    if let Some(draw) = result
        .draw
        .as_ref()
        .filter(|draw| draw.status == FileStatus::Changed)
    {
        let mut part = String::from("the draw text changed");
        if let Some(diff) = &draw.diff {
            part.push_str(&format!(" (diff: {})", shown(diff)));
        }
        parts.push(part);
    }
    match parts.is_empty() {
        true => String::from("it no longer matches its goldens"),
        false => format!(
            "{}; accept it with `cargo telar test --update-snapshots`",
            parts.join("; ")
        ),
    }
}

#[cfg(test)]
#[path = "snapshot_check_test.rs"]
mod tests;
