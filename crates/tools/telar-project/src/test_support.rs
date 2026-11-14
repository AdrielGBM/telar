use std::ffi::OsString;

/// A file name ending in `suffix` that is not valid UTF-8, spelled as each platform's file names can be.
#[cfg(unix)]
pub(crate) fn non_utf8_name(suffix: &str) -> OsString {
    use std::os::unix::ffi::OsStringExt;
    let mut name = b"caf\xe9".to_vec();
    name.extend_from_slice(suffix.as_bytes());
    OsString::from_vec(name)
}

/// A file name ending in `suffix` that is not valid UTF-8, spelled as each platform's file names can be.
#[cfg(windows)]
pub(crate) fn non_utf8_name(suffix: &str) -> OsString {
    use std::os::windows::ffi::OsStringExt;
    let lone_surrogate = 0xd800;
    let name: Vec<u16> = "caf"
        .encode_utf16()
        .chain([lone_surrogate])
        .chain(suffix.encode_utf16())
        .collect();
    OsString::from_wide(&name)
}
