use super::*;
use crate::test_support::non_utf8_name;

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

#[test]
fn refuses_a_component_that_is_not_utf8() {
    let base = Path::new("/pkg/src");
    let name = non_utf8_name(".rsx");
    assert_eq!(relative_slashed(&base.join(&name), base), None);
    assert_eq!(
        relative_slashed(&base.join(&name).join("x.rsx"), base),
        None
    );
}

#[test]
fn names_every_component_of_a_utf8_path() {
    assert_eq!(
        utf8_components(Path::new("ui/forms/field.rsx")),
        Ok(vec!["ui", "forms", "field.rsx"])
    );
}

#[test]
fn a_path_that_is_not_utf8_is_refused_naming_where_it_stops_being_utf8() {
    let name = non_utf8_name("");
    let path = Path::new("ui").join(&name).join("field.rsx");

    let error = utf8_components(&path).unwrap_err();

    assert_eq!(error.path, Path::new("ui").join(&name));
    assert!(error.to_string().contains("not named in UTF-8"), "{error}");
}
