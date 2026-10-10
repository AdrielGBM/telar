use std::collections::HashMap;
use std::path::PathBuf;

use super::*;

fn options(vars: &[(&str, &str)]) -> Result<TestOptions, OptionsError> {
    let vars: HashMap<String, String> = vars
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    TestOptions::from_vars(|name| vars.get(name).cloned())
}

#[test]
fn a_run_nobody_configured_checks_every_preview_and_takes_no_snapshot() {
    let options = options(&[]).unwrap();
    assert_eq!(options, TestOptions::default());
    assert_eq!(options.a11y, A11yLevel::Warn);
    assert_eq!(options.snapshots, SnapshotSetting::Off);
}

#[test]
fn every_variable_cargo_telar_sets_is_read() {
    let options = options(&[
        (TEST_PREVIEW_VAR, " demo--button--* "),
        (TEST_A11Y_VAR, "error"),
        (TEST_SNAPSHOTS_VAR, "update"),
        (TEST_PACKAGE_DIR_VAR, "/repo/apps/demo"),
        (TEST_TARGET_DIR_VAR, "/repo/target"),
        (TEST_TOLERANCE_CHANNEL_VAR, "4"),
        (TEST_TOLERANCE_PIXELS_VAR, "20"),
    ])
    .unwrap();
    assert_eq!(options.filter, Some(PreviewFilter::new("demo--button--*")));
    assert_eq!(options.a11y, A11yLevel::Error);
    assert_eq!(options.snapshots, SnapshotSetting::Update);
    assert_eq!(options.package_dir, Some(PathBuf::from("/repo/apps/demo")));
    assert_eq!(options.target_dir, Some(PathBuf::from("/repo/target")));
    assert_eq!(options.channel_tolerance, Some(4));
    assert_eq!(options.max_differing_pixels, Some(20));
}

#[test]
fn a_blank_variable_counts_as_unset() {
    let options = options(&[(TEST_PREVIEW_VAR, "  "), (TEST_A11Y_VAR, "")]).unwrap();
    assert_eq!(options.filter, None);
    assert_eq!(options.a11y, A11yLevel::Warn);
}

#[test]
fn a_value_a_run_does_not_understand_is_refused_by_name() {
    let error = options(&[(TEST_A11Y_VAR, "loud")]).unwrap_err();
    assert_eq!(error.var, TEST_A11Y_VAR);
    assert!(error.to_string().contains("\"loud\""), "{error}");
    assert!(options(&[(TEST_SNAPSHOTS_VAR, "sometimes")]).is_err());
    assert!(options(&[(TEST_TOLERANCE_CHANNEL_VAR, "300")]).is_err());
    assert!(options(&[(TEST_TOLERANCE_PIXELS_VAR, "-1")]).is_err());
}

#[test]
fn each_word_reads_back_as_itself() {
    for level in [A11yLevel::Off, A11yLevel::Warn, A11yLevel::Error] {
        assert_eq!(A11yLevel::parse(level.as_str()), Some(level));
    }
    for setting in [
        SnapshotSetting::Off,
        SnapshotSetting::Compare,
        SnapshotSetting::Update,
    ] {
        assert_eq!(SnapshotSetting::parse(setting.as_str()), Some(setting));
    }
}

#[test]
fn a_glob_matches_a_whole_id() {
    let filter = PreviewFilter::new("demo--button--primary");
    assert!(filter.matches("demo--button--primary"));
    assert!(!filter.matches("demo--button--primary--mode=dark"));
    assert!(!filter.matches("demo--button--prim"));
}

#[test]
fn a_star_stands_for_any_run_and_a_question_mark_for_one_character() {
    let filter = PreviewFilter::new("demo--*--primary*");
    assert!(filter.matches("demo--button--primary"));
    assert!(filter.matches("demo--icon-button--primary--mode=dark"));
    assert!(!filter.matches("demo--button--secondary"));
    assert!(PreviewFilter::new("*").matches(""));
    assert!(PreviewFilter::new("demo--card--?").matches("demo--card--a"));
    assert!(!PreviewFilter::new("demo--card--?").matches("demo--card--ab"));
    assert!(PreviewFilter::new("*a*a*a").matches("aaaa"));
    assert!(!PreviewFilter::new("*a*b").matches("aaaa"));
    assert!(PreviewFilter::new("dé*").matches("démo"));
}
