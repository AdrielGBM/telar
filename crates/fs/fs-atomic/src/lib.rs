//! Replacing a file's contents all at once: a reader finds the previous contents or the new ones, never part of either.

use std::fs::Permissions;
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

#[cfg(not(windows))]
use std::fs::rename as rename_over;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::rename_over;

/// Writes `contents` to `path` through a file of its own beside it renamed over it, so a run that dies mid-write leaves the previous file rather than half of one, and two writers never share the file they write through.
///
/// Otherwise the result is what writing in place leaves: the directories above `path` are created, a symlink stays a symlink to the file it names and that file is the one replaced, a replaced file keeps its permissions, read-only included, and a new one gets what the umask allows. A file another handle holds open is replaced all the same, on Windows too.
pub fn write_atomically(path: &Path, contents: &[u8]) -> io::Result<()> {
    let target = followed(path)?;
    let dir = target.parent().unwrap_or(Path::new(""));
    std::fs::create_dir_all(dir)?;
    let kept = permissions_of(&target)?;
    let mut file = file_beside(dir)?;
    file.write_all(contents)?;
    // Kept before its permissions are set: on Windows, keeping the file resets its attributes, read-only among them.
    let temp = file.into_temp_path().keep()?;
    put_in_place(&temp, &target, kept).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })
}

fn followed(path: &Path) -> io::Result<PathBuf> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => std::fs::canonicalize(path),
        Ok(_) => Ok(path.to_path_buf()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(error) => Err(error),
    }
}

fn permissions_of(path: &Path) -> io::Result<Option<Permissions>> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(Some(metadata.permissions())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn put_in_place(temp: &Path, target: &Path, kept: Option<Permissions>) -> io::Result<()> {
    if let Some(permissions) = kept {
        std::fs::set_permissions(temp, permissions)?;
    }
    rename_over(temp, target)
}

fn file_beside(dir: &Path) -> io::Result<tempfile::NamedTempFile> {
    with_umask_mode(tempfile::Builder::new()).tempfile_in(dir)
}

// A temporary file is created owner-only, while a new file is expected to be readable as one written in place would be: whatever the umask allows.
#[cfg(unix)]
fn with_umask_mode<'a, 'b>(mut builder: tempfile::Builder<'a, 'b>) -> tempfile::Builder<'a, 'b> {
    use std::os::unix::fs::PermissionsExt;
    builder.permissions(Permissions::from_mode(0o666));
    builder
}

#[cfg(not(unix))]
fn with_umask_mode<'a, 'b>(builder: tempfile::Builder<'a, 'b>) -> tempfile::Builder<'a, 'b> {
    builder
}

#[cfg(test)]
#[path = "lib.test.rs"]
mod tests;
