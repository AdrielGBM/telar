use super::*;

#[test]
fn two_directories_never_share_a_path() {
    let first = scratch_dir("test").unwrap();
    let second = scratch_dir("test").unwrap();
    assert_ne!(first.path(), second.path());
}

#[cfg(unix)]
#[test]
fn the_directory_is_private_to_the_user() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch_dir("test").unwrap();
    let mode = std::fs::metadata(dir.path()).unwrap().permissions().mode();
    assert_eq!(mode & 0o077, 0);
}

#[cfg(unix)]
#[test]
fn a_planted_symlink_at_the_old_predictable_name_is_not_followed() {
    let victim = scratch_dir("victim").unwrap();
    let old_name = std::env::temp_dir().join(format!("telar-test-{}", std::process::id()));
    let _ = std::fs::remove_file(&old_name);
    std::os::unix::fs::symlink(victim.path(), &old_name).unwrap();
    let dir = scratch_dir("test").unwrap();
    std::fs::write(dir.path().join("page-0.json"), b"{}").unwrap();
    let _ = std::fs::remove_file(&old_name);
    assert!(!dir.path().is_symlink());
    assert_eq!(std::fs::read_dir(victim.path()).unwrap().count(), 0);
}
