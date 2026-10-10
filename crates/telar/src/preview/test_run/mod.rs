//! Testing previews, as `cargo telar test` does in the app it builds: each preview, and each cell of its matrix, built and laid out on a canvas of its own, played, checked for accessibility and compared against its snapshots, with a line printed for each and `report.json` written for the run.

mod options;
mod report;

#[cfg(feature = "preview-snapshots")]
#[path = "snapshots.rs"]
mod snapshots;
#[cfg(not(feature = "preview-snapshots"))]
#[path = "no_snapshots.rs"]
mod snapshots;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use crate::AppConfig;

use super::a11y::Severity;
use super::host::duplicate_ids;
use super::{Matrices, MatrixCell, Play, PlayStep, PreviewEntry};

pub use options::{A11yLevel, OptionsError, PreviewFilter, SnapshotSetting, TestOptions};
#[cfg(feature = "preview-snapshots")]
pub use report::TestReport;
pub use report::{
    A11yCheck, A11yFinding, BuildCheck, CellReport, CheckStatus, PlayCheck, PlayStepReport,
    PreviewReport, SnapshotCheck, TestRun, TestSummary,
};

use snapshots::SnapshotRun;

/// Tests `entries` as `options` asks, printing a line for each preview and cell as it goes, and writes `report.json` when the options name a target directory.
///
/// Snapshots, when there are any, make the faces `config` declares the only ones text sets in, so this is called before anything in the process measures text.
///
/// The run is the host every canvas without a locale of its own follows, so it is in the language the app's text is in: the locale the app's setup chose, else [`i18n_core::default_locale`].
pub fn run_previews(
    entries: Vec<PreviewEntry>,
    config: &AppConfig,
    options: &TestOptions,
) -> TestRun {
    let mut run = TestRun {
        filter: options
            .filter
            .as_ref()
            .map(|filter| filter.glob().to_string()),
        a11y: options.a11y,
        snapshots: options.snapshots,
        ..TestRun::default()
    };
    speak_the_app_language();
    let mut snapshots = start_snapshots(config, options, &mut run);
    if snapshots.is_none() {
        crate::runner::install_preview_text_metrics(config);
    }
    ui_core::open_surface_font_family(
        config
            .font_family
            .as_deref()
            .map(renderer_core::FontFamily::from),
    );
    if let Some(duplicates) = duplicate_ids(&entries) {
        run.errors.extend(duplicates.lines().map(str::to_string));
    }

    let matrices = Matrices::installed();
    let planned = plan(&entries, &matrices, options.filter.as_ref());
    let checks: usize = planned.iter().map(|(_, units)| units.len()).sum();
    println!(
        "running {} preview(s), {checks} check(s)  [a11y: {}, snapshots: {}]",
        planned.len(),
        run.a11y.as_str(),
        run.snapshots.as_str()
    );
    for error in &run.errors {
        print_failure(error);
    }
    for (entry, units) in planned {
        let mut preview = preview_report(entry);
        for unit in units {
            let cell = match unit {
                Unit::Preview => check(entry, None, options.a11y, snapshots.as_mut()),
                Unit::Cell(cell) => check(entry, Some(&cell), options.a11y, snapshots.as_mut()),
                Unit::Unexpandable(error) => unexpandable(entry, error),
            };
            print_cell(&cell);
            preview.cells.push(cell);
        }
        run.previews.push(preview);
    }
    if let Some(filter) = &options.filter
        && run.previews.is_empty()
    {
        let error = format!("no preview or matrix cell id matches `{}`", filter.glob());
        print_failure(&error);
        run.errors.push(error);
    }

    run.summarize();
    match snapshots::write_report(snapshots, &run, options) {
        Ok(Some(path)) => {
            println!();
            println!("report: {}", shown(&path));
        }
        Ok(None) => {}
        Err(error) => {
            print_failure(&error);
            run.errors.push(error);
            run.summarize();
        }
    }
    print_summary(&run.summary);
    run
}

/// Runs [`run_previews`] with the options `cargo telar test` set, and fails the test calling it when a preview fails: what `rsx_modules!` emits as a library's `#[test] fn telar_previews()`, since a library has no binary to run.
#[doc(hidden)]
pub fn __test_library(entries: Vec<PreviewEntry>, config: AppConfig) {
    let options = TestOptions::from_env().unwrap_or_else(|error| panic!("{error}"));
    let run = run_previews(entries, &config, &options);
    assert!(
        run.passed(),
        "{} preview check(s) failed; each is listed above",
        run.summary.failed
    );
}

fn speak_the_app_language() {
    if i18n_core::current_locale().is_none() {
        i18n_core::set_locale(i18n_core::default_locale());
    }
}

fn start_snapshots(
    config: &AppConfig,
    options: &TestOptions,
    run: &mut TestRun,
) -> Option<SnapshotRun> {
    if options.snapshots == SnapshotSetting::Off {
        return None;
    }
    if crate::runner::previews_measure_in_cells() {
        println!(
            "note: a terminal build measures text in cells and draws no picture to compare, so snapshots are off"
        );
        run.snapshots = SnapshotSetting::Off;
        return None;
    }
    snapshots::start(config, options)
        .map_err(|error| run.errors.push(error))
        .ok()
}

/// What a run checks of one preview.
enum Unit {
    /// The preview as it opens.
    Preview,
    Cell(MatrixCell),
    /// A matrix that has no cells to give, which fails as one check named `<preview-id>--matrix`.
    Unexpandable(String),
}

/// The previews a run checks, in order, each with what it checks of them: the preview and every cell of its matrix, or, when the filter names neither the preview nor any of its cells, nothing; when it names cells but not the preview, those cells alone.
fn plan<'a>(
    entries: &'a [PreviewEntry],
    matrices: &Matrices,
    filter: Option<&PreviewFilter>,
) -> Vec<(&'a PreviewEntry, Vec<Unit>)> {
    entries
        .iter()
        .filter_map(|entry| {
            let whole = filter.is_none_or(|filter| filter.matches(entry.id));
            let cells = entry.matrix.map(|matrix| matrix.cells(entry, matrices));
            let units: Vec<Unit> = match (whole, cells) {
                (true, None) => vec![Unit::Preview],
                (true, Some(Ok(cells))) => std::iter::once(Unit::Preview)
                    .chain(cells.into_iter().map(Unit::Cell))
                    .collect(),
                (true, Some(Err(error))) => {
                    vec![Unit::Preview, Unit::Unexpandable(error.to_string())]
                }
                (false, Some(Ok(cells))) => cells
                    .into_iter()
                    .filter(|cell| filter.is_some_and(|filter| filter.matches(&cell.id)))
                    .map(Unit::Cell)
                    .collect(),
                (false, _) => Vec::new(),
            };
            (!units.is_empty()).then_some((entry, units))
        })
        .collect()
}

fn preview_report(entry: &PreviewEntry) -> PreviewReport {
    PreviewReport {
        id: entry.id.to_string(),
        title: entry.title.to_string(),
        component: entry.component.to_string(),
        name: entry.name.to_string(),
        file: entry.file.to_string(),
        line: entry.line,
        cells: Vec::new(),
    }
}

fn unexpandable(entry: &PreviewEntry, error: String) -> CellReport {
    let mut report = CellReport::new(format!("{}--matrix", entry.id), Vec::new());
    report.build = BuildCheck {
        status: CheckStatus::Failed,
        draw_commands: 0,
        error: Some(format!("the matrix has no cells to give: {error}")),
    };
    report.settle()
}

/// Builds and lays out `entry`, in `cell` when there is one, then runs its play, checks the canvas the play left, and compares the canvas as it opened with its snapshot.
fn check(
    entry: &PreviewEntry,
    cell: Option<&MatrixCell>,
    a11y: A11yLevel,
    snapshots: Option<&mut SnapshotRun>,
) -> CellReport {
    let id = cell.map_or_else(|| entry.id.to_string(), |cell| cell.id.clone());
    let mut report = CellReport::new(id, MatrixCell::owned_coordinates(cell));
    let mut play = match guarded(|| mount(entry, cell)).and_then(|mounted| mounted) {
        Ok(play) => play,
        Err(error) => {
            if let Some(snapshots) = snapshots {
                snapshots.unbuilt(entry, cell, &error);
                report.snapshot.detail = Some(String::from("not taken: the preview did not build"));
            }
            report.build = BuildCheck {
                status: CheckStatus::Failed,
                draw_commands: 0,
                error: Some(error),
            };
            return report.settle();
        }
    };
    let opened = play.frame();
    report.build = BuildCheck {
        status: CheckStatus::Passed,
        draw_commands: opened.commands.len(),
        error: None,
    };
    report.play = run_play(&mut play);
    report.a11y = check_a11y(&play, a11y);
    if let Some(snapshots) = snapshots {
        report.snapshot =
            guarded(|| snapshots.check(entry, cell, &opened)).unwrap_or_else(|error| {
                SnapshotCheck {
                    status: CheckStatus::Failed,
                    detail: Some(error),
                }
            });
    }
    let _ = guarded(move || drop(play));
    report.settle()
}

fn mount(entry: &PreviewEntry, cell: Option<&MatrixCell>) -> Result<Play, String> {
    match cell {
        None => Play::mount(entry),
        Some(cell) => {
            let ctx = cell
                .ctx(entry)
                .map_err(|error| format!("the matrix cell could not be set up: {error}"))?;
            Play::mount_with(entry, ctx, Play::DEFAULT_VIEWPORT)
        }
    }
    .map_err(|error| error.to_string())
}

fn run_play(play: &mut Play) -> PlayCheck {
    if play.entry().play.is_none() {
        return PlayCheck::default();
    }
    let outcome = guarded(|| play.run());
    let steps = play.steps().iter().map(step_report).collect();
    let (status, error) = match outcome {
        Ok(Ok(())) => (CheckStatus::Passed, None),
        Ok(Err(error)) => (CheckStatus::Failed, Some(error.message)),
        Err(panic) => (CheckStatus::Failed, Some(panic)),
    };
    PlayCheck {
        status,
        steps,
        error,
    }
}

fn step_report(step: &PlayStep) -> PlayStepReport {
    PlayStepReport {
        action: step.action.clone(),
        error: step
            .outcome
            .as_ref()
            .err()
            .map(|error| error.message.clone()),
    }
}

fn check_a11y(play: &Play, level: A11yLevel) -> A11yCheck {
    if level == A11yLevel::Off {
        return A11yCheck::default();
    }
    let failing = |severity| level == A11yLevel::Error && severity == Severity::Error;
    match guarded(|| play.check_a11y()) {
        Ok(found) => A11yCheck {
            status: match found.worst() {
                None => CheckStatus::Passed,
                Some(worst) if failing(worst) => CheckStatus::Failed,
                Some(_) => CheckStatus::Warned,
            },
            violations: found.violations.iter().map(A11yFinding::of).collect(),
            error: None,
        },
        Err(error) => A11yCheck {
            status: match failing(Severity::Error) {
                true => CheckStatus::Failed,
                false => CheckStatus::Warned,
            },
            violations: Vec::new(),
            error: Some(error),
        },
    }
}

/// Runs `f`, turning a panic into the message it panicked with: one preview that panics fails its own check and the run goes on.
fn guarded<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|panic| {
        let message = panic
            .downcast_ref::<&str>()
            .map(|message| message.to_string())
            .or_else(|| panic.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| String::from("a value that is not a message"));
        format!("panicked: {message}")
    })
}

/// `path` relative to the directory the run was started from, where it is inside it: what a person reads in a terminal and a CI log.
fn shown(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

fn print_failure(error: &str) {
    println!("  FAIL  {error}");
}

fn print_cell(cell: &CellReport) {
    let word = match (cell.passed, cell.a11y.status) {
        (false, _) => "FAIL",
        (true, CheckStatus::Warned) => "warn",
        (true, _) => "ok",
    };
    let summary = cell_summary(cell);
    match summary.is_empty() {
        true => println!("  {word:<4}  {}", cell.id),
        false => println!("  {word:<4}  {}  ({})", cell.id, summary.join(", ")),
    }
    for line in cell.details() {
        let mut lines = line.lines();
        if let Some(first) = lines.next() {
            println!("          {first}");
        }
        for rest in lines {
            println!("            {rest}");
        }
    }
}

fn cell_summary(cell: &CellReport) -> Vec<String> {
    let mut parts = Vec::new();
    if cell.build.status == CheckStatus::Passed {
        parts.push(format!("{} draw commands", cell.build.draw_commands));
    }
    if cell.play.status == CheckStatus::Passed {
        parts.push(format!("play: {} step(s)", cell.play.steps.len()));
    }
    if cell.snapshot.status == CheckStatus::Passed
        && let Some(detail) = &cell.snapshot.detail
    {
        parts.push(format!("snapshot {detail}"));
    }
    if !cell.a11y.violations.is_empty() {
        parts.push(format!("{} a11y finding(s)", cell.a11y.violations.len()));
    }
    parts
}

fn print_summary(summary: &TestSummary) {
    println!();
    let warned = match summary.warned {
        0 => String::new(),
        warned => format!(", {warned} with a11y findings"),
    };
    println!(
        "test result: {} passed, {} failed ({} preview(s), {} check(s){warned})",
        summary.passed, summary.failed, summary.previews, summary.checked
    );
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
