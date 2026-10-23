use tempfile::TempDir;

/// A directory only this user can enter, named at random and created exclusively, so no other user on a shared host can plant the path first or read what is written there.
pub(super) fn scratch_dir(purpose: &str) -> Result<TempDir, String> {
    tempfile::Builder::new()
        .prefix(&format!("telar-{purpose}-"))
        .permissions(owner_only())
        .tempdir()
        .map_err(|e| {
            format!(
                "could not create a private scratch directory for {purpose} in {}: {e}",
                std::env::temp_dir().display()
            )
        })
}

#[cfg(test)]
#[path = "scratch_test.rs"]
mod tests;

#[cfg(unix)]
fn owner_only() -> std::fs::Permissions {
    use std::os::unix::fs::PermissionsExt;
    std::fs::Permissions::from_mode(0o700)
}

#[cfg(not(unix))]
fn owner_only() -> std::fs::Permissions {
    std::fs::Permissions::from(
        std::fs::metadata(std::env::temp_dir())
            .unwrap()
            .permissions(),
    )
}
