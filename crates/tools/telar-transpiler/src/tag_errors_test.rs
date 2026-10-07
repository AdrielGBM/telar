use super::*;

fn rewrite_tag_error(
    code: &str,
    generated: &str,
    span: (usize, usize),
    prelude: &[PreludeEntry],
    candidates: &[String],
) -> Option<String> {
    tag_error_message(
        code,
        &generated_site(generated, span.0, span.1)?,
        prelude,
        candidates,
    )
}

fn prelude(entries: &[&str]) -> Vec<PreludeEntry> {
    entries
        .iter()
        .map(|entry| PreludeEntry::parse(entry).unwrap())
        .collect()
}

/// Real transpiler output for `view`, against `entries` as the prelude, so the recogniser is tested against what the emitter writes today rather than against a copy of it.
fn generated(view: &str, entries: &[&str]) -> String {
    let prelude = prelude(entries);
    crate::transpile_buffer(
        std::path::Path::new("/p/src/home.rsx"),
        &format!("[view]\n{view}\n"),
        &crate::PackageOptions {
            src_dir: std::path::Path::new("/p/src"),
            theme_type: None,
            assets: None,
            prelude: &prelude,
            library: false,
            flavour: telar_project::BuildFlavour::Plain,
        },
    )
    .unwrap()
    .rust_code
}

fn span_of(code: &str, needle: &str) -> (usize, usize) {
    let at = code
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is not in:\n{code}"));
    (at, at + needle.len())
}

#[test]
fn a_span_on_a_calls_tag_or_props_type_is_that_tag() {
    let code = generated("column\n    buton label:\"Go\"", &[]);
    for needle in ["buton(", "ButonProps"] {
        let (start, _) = span_of(&code, needle);
        let end = start + needle.trim_end_matches('(').len();
        assert_eq!(
            generated_site(&code, start, end),
            Some(GeneratedSite::Tag {
                tag: "buton",
                name: needle.trim_end_matches('(')
            }),
            "{code}"
        );
    }
}

/// A call with children is emitted without a `let`, inside the block that builds them.
#[test]
fn a_call_with_children_is_recognised_too() {
    let code = generated("panel\n    text \"inside\"", &[]);
    let (start, end) = span_of(&code, "PanelProps");
    assert_eq!(
        generated_site(&code, start, end),
        Some(GeneratedSite::Tag {
            tag: "panel",
            name: "PanelProps"
        }),
        "{code}"
    );
}

#[test]
fn a_path_tag_reports_the_segment_under_the_span() {
    let code = generated("topbar::strip", &[]);
    let (start, end) = span_of(&code, "topbar");
    assert_eq!(
        generated_site(&code, start, end),
        Some(GeneratedSite::Tag {
            tag: "topbar::strip",
            name: "topbar"
        }),
        "{code}"
    );
}

/// An error inside an attribute value on the same line is about that value, not about the tag.
#[test]
fn a_span_elsewhere_on_the_call_line_is_not_the_tag() {
    let code = generated("button label:(missing_name)", &[]);
    let (start, end) = span_of(&code, "missing_name");
    assert_eq!(generated_site(&code, start, end), None, "{code}");
}

#[test]
fn the_glob_imports_are_recognised_by_path() {
    let code = generated("text \"x\"", &["telar-components", "my_plugin::prelude"]);
    for path in ["telar", "telar_components", "my_plugin::prelude", "crate"] {
        let (start, end) = span_of(&code, &format!("{path}::*"));
        assert_eq!(
            generated_site(&code, start, end),
            Some(GeneratedSite::Glob(path)),
            "{code}"
        );
    }
}

#[test]
fn an_unknown_tag_lists_the_prelude_it_was_looked_up_in() {
    let code = generated("buton", &[]);
    let span = span_of(&code, "buton");

    let none = rewrite_tag_error("E0425", &code, span, &[], &[]).unwrap();
    assert_eq!(
        none,
        "unknown tag `buton`: not a built-in, not a `.rsx` in this package, and not exported by any `[telar] prelude` entry (this package declares none)"
    );
    let some = rewrite_tag_error(
        "E0433",
        &code,
        span_of(&code, "ButonProps"),
        &prelude(&["telar-components", "telar-navigate"]),
        &[],
    )
    .unwrap();
    assert!(
        some.ends_with("(`telar_components`, `telar_navigate`)"),
        "{some}"
    );
}

/// The advice imports both names a bare tag is, since both clash and settling one leaves the other an error.
#[test]
fn a_clash_names_both_crates_and_the_use_that_settles_it() {
    let code = generated("button", &["telar-components"]);
    let message = rewrite_tag_error(
        "E0659",
        &code,
        span_of(&code, "ButtonProps"),
        &[],
        &["telar".to_string(), "telar_components".to_string()],
    )
    .unwrap();
    assert_eq!(
        message,
        "tag `button` is exported by both `telar` and `telar_components`; pick one with `use telar::{button, ButtonProps};` in `[logic]`"
    );
    assert_eq!(
        rewrite_tag_error("E0659", &code, span_of(&code, "button("), &[], &[]),
        None,
        "with no candidates there is nothing better to say than rustc's own message"
    );
}

#[test]
fn other_errors_at_a_tag_keep_rustcs_wording() {
    let code = generated("button", &[]);
    assert_eq!(
        rewrite_tag_error("E0308", &code, span_of(&code, "button("), &[], &[]),
        None
    );
}

#[test]
fn the_tag_is_found_where_its_line_opens() {
    assert_eq!(tag_columns("    buton label:\"Go\"", "buton"), Some((4, 9)));
    assert_eq!(tag_columns("topbar::strip", "topbar::strip"), Some((0, 13)));
    assert_eq!(tag_columns("    button_row", "button"), None);
    assert_eq!(tag_columns("    topbar::strip", "topbar"), None);
    assert_eq!(tag_columns("    text \"button\"", "button"), None);
}

/// The file on disk can change between rustc compiling it and this reading it back, so an offset may land anywhere.
#[test]
fn an_offset_past_the_end_or_inside_a_character_is_no_site() {
    let code = generated("text \"é\"", &[]);
    assert_eq!(generated_site(&code, code.len() + 1, code.len() + 2), None);
    let inside = code.find('é').unwrap() + 1;
    assert_eq!(generated_site(&code, inside, inside), None);
}
