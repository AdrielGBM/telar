use super::*;

struct StateAt(Option<PathBuf>);

impl AppPathsProvider for StateAt {
    fn config_dir(&self) -> Option<PathBuf> {
        None
    }
    fn data_dir(&self) -> Option<PathBuf> {
        None
    }
    fn cache_dir(&self) -> Option<PathBuf> {
        None
    }
    fn state_dir(&self) -> Option<PathBuf> {
        self.0.clone()
    }
}

#[test]
fn log_file_lives_under_the_state_dir_by_app_name() {
    let paths = StateAt(Some(PathBuf::from("/state")));
    assert_eq!(
        log_file_path(None, &paths, "notes"),
        Some(PathBuf::from("/state/notes/notes.log"))
    );
}

#[test]
fn log_file_override_wins_and_empty_is_ignored() {
    let paths = StateAt(Some(PathBuf::from("/state")));
    assert_eq!(
        log_file_path(Some("/tmp/x.log".into()), &paths, "notes"),
        Some(PathBuf::from("/tmp/x.log"))
    );
    assert_eq!(
        log_file_path(Some(OsString::new()), &paths, "notes"),
        Some(PathBuf::from("/state/notes/notes.log"))
    );
}

#[test]
fn log_file_override_needs_no_state_dir() {
    assert_eq!(
        log_file_path(Some("/tmp/x.log".into()), &StateAt(None), "notes"),
        Some(PathBuf::from("/tmp/x.log"))
    );
    assert_eq!(log_file_path(None, &StateAt(None), "notes"), None);
}

#[test]
fn opening_creates_missing_parents_and_appends() {
    let dir = std::env::temp_dir().join(format!("telar-log-test-{}", std::process::id()));
    let path = dir.join("a").join("b.log");
    {
        use std::io::Write;
        writeln!(open_log_file(&path).unwrap(), "one").unwrap();
        writeln!(open_log_file(&path).unwrap(), "two").unwrap();
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one\ntwo\n");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn filter_defaults_to_warn_and_survives_a_bad_directive() {
    assert_eq!(filter_from(None).to_string(), "warn");
    assert_eq!(filter_from(Some("")).to_string(), "warn");
    assert_eq!(filter_from(Some("telar=debug")).to_string(), "telar=debug");
    assert_eq!(filter_from(Some("=[bad")).to_string(), "warn");
}
