//! `cargo telar test`: every preview and matrix cell of a package, built, played, checked for accessibility and compared against its snapshots, by the app itself or, for a library, by the unit test `rsx_modules!` gives it.

use std::path::{Path, PathBuf};
use std::process::Command;

use telar_project::SnapshotSettings;
use telar_project::protocol::{
    LIBRARY_TEST_NAME, TEST_A11Y_VAR, TEST_OUT_DIR, TEST_PACKAGE_DIR_VAR, TEST_PREVIEW_VAR,
    TEST_REPORT_FILE, TEST_SNAPSHOTS_VAR, TEST_TARGET_DIR_VAR, TEST_TOLERANCE_CHANNEL_VAR,
    TEST_TOLERANCE_PIXELS_VAR, TEST_VAR,
};

use super::cli::{A11yArg, CommonArgs, Target, TestArgs};
use super::config::{self, load_config, resolve_package};
use super::junit::junit_from_report;
use super::{build_cargo_args, frontend_args};

/// What `cargo telar test` turns on: the `[preview]` blocks, the runner that tests them, and the rasterizer and report its snapshots need. One feature whatever the flags, so `--no-snapshots` costs no rebuild.
const TEST_FEATURE: &str = "telar/preview-snapshots";

pub(super) fn run_test_cmd(args: TestArgs) -> ! {
    let TestArgs {
        common,
        release,
        preview,
        a11y,
        update_snapshots,
        no_snapshots,
        report,
    } = args;
    let report = match report.as_deref().map(ReportRequest::parse).transpose() {
        Ok(report) => report,
        Err(error) => {
            eprintln!("[cargo-telar] {error}");
            std::process::exit(2);
        }
    };
    let CommonArgs {
        package,
        features,
        target,
        backend,
        renderer,
        cargo_args: extra,
    } = common;
    if let Some(target @ (Target::Android | Target::Web)) = target {
        let name = match target {
            Target::Android => "android",
            _ => "web",
        };
        eprintln!(
            "[cargo-telar] `cargo telar test` renders on the host; --target {name} is not supported."
        );
        std::process::exit(2);
    }
    let mut cargo_args = build_cargo_args(&package, release, &features);
    cargo_args.extend(extra);
    // The frontend the target named, so a terminal project's tests are not a desktop build — and so `--target tui` reaches this command too, rather than every command but this one.
    if let Some(target) = target {
        let selected = frontend_args(target, None, &cargo_args);
        cargo_args.extend(selected);
    }
    let resolved = resolve_package(&cargo_args);
    let section = load_config(&cargo_args);
    let target_dir = telar_project::find_target_dir(&resolved.workspace_root);
    config::warn_if_tooling_unlocked(&cargo_args, &[TEST_FEATURE]);
    cargo_args.push("--features".to_string());
    cargo_args.push(TEST_FEATURE.to_string());
    // A snapshot is drawn on the CPU rasterizer whatever these name, and the value is read via `option_env!`, so setting it would change the build fingerprint and force a needless recompile.
    let _ = (backend, renderer);

    let snapshots = match (no_snapshots, update_snapshots) {
        (true, _) => "off",
        (false, true) => "update",
        (false, false) => "compare",
    };
    let env = TestEnv {
        filter: preview,
        a11y,
        snapshots,
        package_dir: resolved.package_dir.clone(),
        target_dir: target_dir.clone(),
        settings: section.previews.snapshots.unwrap_or_default(),
    };
    let written = target_dir.join(TEST_OUT_DIR).join(TEST_REPORT_FILE);
    // A build that fails writes no report, and the one an earlier run left would then pass for this run's.
    let _ = std::fs::remove_file(&written);
    let mut command = Command::new("cargo");
    command.args(invocation(section.library, &cargo_args));
    for (name, value) in env.vars() {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    if section.library {
        eprintln!("[cargo-telar] Testing the previews of a library through `cargo test`...");
    } else {
        eprintln!("[cargo-telar] Testing previews...");
        command.env(TEST_VAR, "1");
    }
    let status = command
        .status()
        .expect("[cargo-telar] failed to invoke cargo");
    if let Some(report) = report {
        match report.write_from(&written) {
            Ok(()) => eprintln!(
                "[cargo-telar] Wrote the report to {}.",
                report.path.display()
            ),
            Err(error) => eprintln!(
                "[cargo-telar] No report written to {}: {error}",
                report.path.display()
            ),
        }
    }
    std::process::exit(status.code().unwrap_or(1));
}

/// The cargo command that tests the package: running its binary, or, for a library, which has none, the one unit test `rsx_modules!` emits for it.
fn invocation(library: bool, cargo_args: &[String]) -> Vec<String> {
    let mut args = vec![String::from(if library { "test" } else { "run" })];
    args.extend_from_slice(cargo_args);
    if library {
        args.extend(["--lib", "--", LIBRARY_TEST_NAME, "--exact", "--nocapture"].map(String::from));
    }
    args
}

/// What the run is asked, as the variables the app reads it from.
struct TestEnv {
    filter: Option<String>,
    a11y: A11yArg,
    snapshots: &'static str,
    package_dir: PathBuf,
    target_dir: PathBuf,
    settings: SnapshotSettings,
}

impl TestEnv {
    /// Each variable with its value, or `None` to clear one the shell may have left set.
    fn vars(&self) -> Vec<(&'static str, Option<String>)> {
        let path = |path: &Path| Some(path.display().to_string());
        vec![
            (TEST_PREVIEW_VAR, self.filter.clone()),
            (TEST_A11Y_VAR, Some(self.a11y.as_str().to_string())),
            (TEST_SNAPSHOTS_VAR, Some(self.snapshots.to_string())),
            (TEST_PACKAGE_DIR_VAR, path(&self.package_dir)),
            (TEST_TARGET_DIR_VAR, path(&self.target_dir)),
            (
                TEST_TOLERANCE_CHANNEL_VAR,
                self.settings
                    .channel_tolerance
                    .map(|channel| channel.to_string()),
            ),
            (
                TEST_TOLERANCE_PIXELS_VAR,
                self.settings
                    .max_differing_pixels
                    .map(|pixels| pixels.to_string()),
            ),
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReportFormat {
    Json,
    Junit,
}

/// `--report <FORMAT> <PATH>`: a copy of the run's report, in a format a CI reads.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ReportRequest {
    format: ReportFormat,
    path: PathBuf,
}

impl ReportRequest {
    fn parse(values: &[String]) -> Result<Self, String> {
        let [format, path] = values else {
            return Err(String::from(
                "--report takes a format and a path: `--report junit out.xml`",
            ));
        };
        let format = match format.as_str() {
            "json" => ReportFormat::Json,
            "junit" => ReportFormat::Junit,
            other => {
                return Err(format!("--report writes `json` or `junit`, not `{other}`"));
            }
        };
        Ok(Self {
            format,
            path: PathBuf::from(path),
        })
    }

    /// Writes the report the run left at `written` to this request's path, in its format.
    fn write_from(&self, written: &Path) -> Result<(), String> {
        let json = std::fs::read_to_string(written).map_err(|error| {
            format!("the run left no report at {} ({error})", written.display())
        })?;
        let contents = match self.format {
            ReportFormat::Json => json,
            ReportFormat::Junit => {
                let report = serde_json::from_str(&json)
                    .map_err(|error| format!("{}: {error}", written.display()))?;
                junit_from_report(&report)?
            }
        };
        if let Some(dir) = self.path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        }
        std::fs::write(&self.path, contents).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
#[path = "test_cmd_test.rs"]
mod tests;
