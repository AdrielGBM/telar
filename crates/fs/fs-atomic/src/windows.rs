use std::io::{self, ErrorKind};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

// The kernel's names for the `FILE_RENAME_FLAG_*` values, the only spelling the bindings carry of the one that ignores the read-only attribute.
use windows_sys::Wdk::Storage::FileSystem::{
    FILE_RENAME_IGNORE_READONLY_ATTRIBUTE, FILE_RENAME_POSIX_SEMANTICS,
    FILE_RENAME_REPLACE_IF_EXISTS,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_RENAME_INFO, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FileRenameInfoEx, SetFileInformationByHandle,
};

// `std::fs::rename` rather than the temporary file's own persist, which is denied a file another handle has open; and since neither replaces a read-only file, a denied rename is retried asking the file system to ignore that attribute.
pub(crate) fn rename_over(temp: &Path, target: &Path) -> io::Result<()> {
    match std::fs::rename(temp, target) {
        Err(error) if error.kind() == ErrorKind::PermissionDenied => {
            rename_ignoring_read_only(temp, target).map_err(|_| error)
        }
        result => result,
    }
}

fn rename_ignoring_read_only(from: &Path, to: &Path) -> io::Result<()> {
    let file = std::fs::OpenOptions::new()
        .access_mode(DELETE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .open(from)?;
    let name: Vec<u16> = std::fs::canonicalize(to)?
        .as_os_str()
        .encode_wide()
        .collect();
    let name_bytes = std::mem::size_of_val(name.as_slice());
    let size = std::mem::offset_of!(FILE_RENAME_INFO, FileName) + name_bytes + size_of::<u16>();
    let too_long = |_| io::Error::from(ErrorKind::InvalidFilename);
    let (name_bytes, size) = (
        u32::try_from(name_bytes).map_err(too_long)?,
        u32::try_from(size).map_err(too_long)?,
    );
    let mut buffer = vec![0u64; (size as usize).div_ceil(size_of::<u64>())];
    let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
    // SAFETY: `buffer` is zeroed, aligned for `FILE_RENAME_INFO`, and long enough for its header, the name and the NUL after it, so every write lands inside it and the root directory stays null.
    unsafe {
        (*info).Anonymous.Flags = FILE_RENAME_REPLACE_IF_EXISTS
            | FILE_RENAME_POSIX_SEMANTICS
            | FILE_RENAME_IGNORE_READONLY_ATTRIBUTE;
        (*info).FileNameLength = name_bytes;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            (&raw mut (*info).FileName).cast::<u16>(),
            name.len(),
        );
    }
    // SAFETY: `info` points at `size` initialised bytes that outlive the call, and the handle is open with the `DELETE` access a rename needs.
    let renamed = unsafe {
        SetFileInformationByHandle(file.as_raw_handle(), FileRenameInfoEx, info.cast(), size)
    };
    match renamed {
        0 => Err(io::Error::last_os_error()),
        _ => Ok(()),
    }
}
