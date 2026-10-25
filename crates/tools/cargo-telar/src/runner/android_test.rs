use super::*;

fn entries_of(env: &str) -> Result<Vec<(String, String)>, String> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".env"), env).unwrap();
    dotenv_entries(dir.path())
}

fn value_of(env: &str, key: &str) -> String {
    entries_of(env)
        .unwrap()
        .into_iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
        .unwrap()
}

#[test]
fn strips_quotes_export_prefix_and_inline_comments() {
    let env = "A=\"quoted value\"\nexport B=plain # trailing\nC='single'\n# whole line\n";
    assert_eq!(value_of(env, "A"), "quoted value");
    assert_eq!(value_of(env, "B"), "plain");
    assert_eq!(value_of(env, "C"), "single");
}

#[test]
fn a_value_naming_an_existing_directory_stays_literal() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("android")).unwrap();
    std::fs::write(
        dir.path().join(".env"),
        "CARGO_APK_RELEASE_KEYSTORE_PASSWORD=android\nEMPTY=\nDOT=.\n",
    )
    .unwrap();
    let entries = dotenv_entries(dir.path()).unwrap();
    assert!(entries.contains(&(
        "CARGO_APK_RELEASE_KEYSTORE_PASSWORD".to_string(),
        "android".to_string()
    )));
    assert!(entries.contains(&("EMPTY".to_string(), String::new())));
    assert!(entries.contains(&("DOT".to_string(), ".".to_string())));
}

#[test]
fn the_keystore_path_is_resolved_against_the_workspace_root() {
    let dir = tempfile::tempdir().unwrap();
    let absolute = dir.path().join("abs").join("dev.keystore");
    std::fs::write(
        dir.path().join(".env"),
        format!(
            "CARGO_APK_RELEASE_KEYSTORE=release.keystore\nCARGO_APK_DEV_KEYSTORE={}\n",
            absolute.display()
        ),
    )
    .unwrap();
    let entries = dotenv_entries(dir.path()).unwrap();
    let release = dir.path().join("release.keystore");
    assert!(entries.contains(&(
        "CARGO_APK_RELEASE_KEYSTORE".to_string(),
        release.to_string_lossy().into_owned()
    )));
    assert!(entries.contains(&(
        "CARGO_APK_DEV_KEYSTORE".to_string(),
        absolute.to_string_lossy().into_owned()
    )));
}

#[test]
fn a_missing_env_file_is_no_entries() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(dotenv_entries(dir.path()), Ok(Vec::new()));
}

#[test]
fn an_unreadable_env_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".env")).unwrap();
    let error = dotenv_entries(dir.path()).unwrap_err();
    assert!(error.contains(".env"), "{error}");
}

#[test]
fn a_malformed_env_file_is_an_error() {
    let error = entries_of("GOOD=1\n=oops\n").unwrap_err();
    assert!(error.contains(".env"), "{error}");
}
