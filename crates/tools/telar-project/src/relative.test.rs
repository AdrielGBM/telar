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
#[test]
fn refuses_a_component_that_is_not_utf8() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let base = Path::new("/pkg/src");
    let name = Path::new(OsStr::from_bytes(b"caf\xe9.rsx"));
    assert_eq!(relative_slashed(&base.join(name), base), None);
    assert_eq!(relative_slashed(&base.join(name).join("x.rsx"), base), None);
}
