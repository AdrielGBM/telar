use clap::Parser;

use super::*;
use crate::runner::cli::{Cli, TelarCommand};

fn test_args(argv: &[&str]) -> Result<TestArgs, clap::Error> {
    let cli = Cli::try_parse_from(
        ["cargo-telar", "test"]
            .iter()
            .chain(argv)
            .map(|arg| arg.to_string()),
    )?;
    match cli.command {
        Some(TelarCommand::Test(args)) => Ok(args),
        _ => panic!("not a test command"),
    }
}

#[test]
fn the_play_lens_names_one_preview_by_id() {
    let args = test_args(&["--preview", "demo--button--primary"]).unwrap();
    assert_eq!(args.preview.as_deref(), Some("demo--button--primary"));
    assert_eq!(args.a11y, A11yArg::Warn);
    assert!(!args.update_snapshots && !args.no_snapshots);
    assert_eq!(args.report, None);
}

#[test]
fn every_flag_of_a_test_run_parses() {
    let args = test_args(&[
        "-p",
        "catalogue",
        "--a11y",
        "error",
        "--update-snapshots",
        "--report",
        "junit",
        "out/previews.xml",
    ])
    .unwrap();
    assert_eq!(args.common.package.as_deref(), Some("catalogue"));
    assert_eq!(args.a11y, A11yArg::Error);
    assert!(args.update_snapshots);
    let report = ReportRequest::parse(args.report.as_deref().unwrap()).unwrap();
    assert_eq!(
        report,
        ReportRequest {
            format: ReportFormat::Junit,
            path: PathBuf::from("out/previews.xml"),
        }
    );
}

#[test]
fn updating_and_skipping_snapshots_are_exclusive() {
    assert!(test_args(&["--update-snapshots", "--no-snapshots"]).is_err());
    assert!(test_args(&["--a11y", "loud"]).is_err());
    assert!(test_args(&["--report", "junit"]).is_err());
}

#[test]
fn a_report_format_is_json_or_junit() {
    let parse = |values: &[&str]| {
        ReportRequest::parse(
            &values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(
        parse(&["json", "r.json"]).unwrap().format,
        ReportFormat::Json
    );
    let error = parse(&["xml", "r.xml"]).unwrap_err();
    assert!(error.contains("`xml`"), "{error}");
}

#[test]
fn an_application_is_run_and_a_library_runs_its_previews_test() {
    let cargo_args = vec![String::from("-p"), String::from("demo")];
    assert_eq!(invocation(false, &cargo_args), ["run", "-p", "demo"]);
    assert_eq!(
        invocation(true, &cargo_args),
        [
            "test",
            "-p",
            "demo",
            "--lib",
            "--",
            "telar_previews",
            "--exact",
            "--nocapture"
        ]
    );
}

#[test]
fn the_run_is_told_everything_through_its_environment() {
    let env = TestEnv {
        filter: None,
        a11y: A11yArg::Off,
        snapshots: "update",
        package_dir: PathBuf::from("/repo/apps/demo"),
        target_dir: PathBuf::from("/repo/target"),
        settings: SnapshotSettings {
            channel_tolerance: Some(3),
            max_differing_pixels: None,
        },
    };
    let vars = env.vars();
    let value = |name: &str| {
        vars.iter()
            .find(|(var, _)| *var == name)
            .unwrap_or_else(|| panic!("{name} is not set"))
            .1
            .clone()
    };
    assert_eq!(
        value(TEST_PREVIEW_VAR),
        None,
        "a filter the shell left set is cleared"
    );
    assert_eq!(value(TEST_A11Y_VAR).as_deref(), Some("off"));
    assert_eq!(value(TEST_SNAPSHOTS_VAR).as_deref(), Some("update"));
    assert_eq!(
        value(TEST_PACKAGE_DIR_VAR).as_deref(),
        Some("/repo/apps/demo")
    );
    assert_eq!(value(TEST_TARGET_DIR_VAR).as_deref(), Some("/repo/target"));
    assert_eq!(value(TEST_TOLERANCE_CHANNEL_VAR).as_deref(), Some("3"));
    assert_eq!(value(TEST_TOLERANCE_PIXELS_VAR), None);
}

#[test]
fn a_requested_report_is_written_from_the_one_the_run_left() {
    let root = std::env::temp_dir().join(format!("cargo_telar_report_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let written = root.join("report.json");
    std::fs::write(
        &written,
        r#"{"test": {"previews": [{"id": "demo--a", "cells": [{"id": "demo--a", "passed": true}]}], "errors": []}}"#,
    )
    .unwrap();

    let json = ReportRequest {
        format: ReportFormat::Json,
        path: root.join("copy/report.json"),
    };
    json.write_from(&written).unwrap();
    assert_eq!(
        std::fs::read_to_string(&json.path).unwrap(),
        std::fs::read_to_string(&written).unwrap()
    );
    let junit = ReportRequest {
        format: ReportFormat::Junit,
        path: root.join("junit.xml"),
    };
    junit.write_from(&written).unwrap();
    let xml = std::fs::read_to_string(&junit.path).unwrap();
    assert!(xml.contains("tests=\"1\" failures=\"0\""), "{xml}");

    let missing = junit.write_from(&root.join("absent.json")).unwrap_err();
    assert!(missing.contains("left no report"), "{missing}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_feature_a_test_run_injects_is_one_a_project_can_lock() {
    assert!(crate::runner::config::TOOLING_FEATURES.contains(&TEST_FEATURE));
}
