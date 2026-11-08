use super::*;

#[test]
fn joins_the_components_under_the_base() {
    let cases: [(&str, &str, Option<&str>); 7] = [
        ("/pkg/src/a/b.rsx", "/pkg/src", Some("a/b.rsx")),
        ("/pkg/src/a.rsx", "/pkg/src", Some("a.rsx")),
        ("/pkg/src", "/pkg/src", Some("")),
        ("/pkg/other/a.rsx", "/pkg/src", None),
        ("/pkg/srcs/a.rsx", "/pkg/src", None),
        ("/pkg/src/../a.rsx", "/pkg/src", Some("../a.rsx")),
        ("/pkg/src/./a.rsx", "/pkg/src", Some("a.rsx")),
    ];
    for (path, base, expected) in cases {
        assert_eq!(
            relative_slashed(Path::new(path), Path::new(base)).as_deref(),
            expected,
            "{path} under {base}"
        );
    }
}

#[cfg(unix)]
#[test]
fn keeps_a_backslash_in_a_unix_file_name() {
    assert_eq!(
        relative_slashed(Path::new("/pkg/src/a\\b.rsx"), Path::new("/pkg/src")).as_deref(),
        Some("a\\b.rsx")
    );
}

#[cfg(unix)]
fn non_utf8_name() -> std::ffi::OsString {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(b"caf\xe9.rsx").to_owned()
}

#[cfg(windows)]
fn non_utf8_name() -> std::ffi::OsString {
    use std::os::windows::ffi::OsStringExt;
    let lone_surrogate = 0xd800;
    let name: Vec<u16> = "caf"
        .encode_utf16()
        .chain([lone_surrogate])
        .chain(".rsx".encode_utf16())
        .collect();
    std::ffi::OsString::from_wide(&name)
}

#[test]
fn refuses_a_component_that_is_not_utf8() {
    let base = Path::new("/pkg/src");
    let name = non_utf8_name();
    assert_eq!(relative_slashed(&base.join(&name), base), None);
    assert_eq!(
        relative_slashed(&base.join(&name).join("x.rsx"), base),
        None
    );
}
