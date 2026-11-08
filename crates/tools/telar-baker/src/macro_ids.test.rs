use super::*;

fn icon_kind() -> &'static AssetKind {
    telar_project::asset_kind_for_id("icon").unwrap()
}

fn ids(source: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    collect_macro_ids(source, Path::new("lib.rs"), 1, &[icon_kind()], &mut out);
    out.into_iter()
        .map(|reference| (reference.literal.unwrap(), reference.line))
        .collect()
}

#[test]
fn a_call_by_its_own_name_or_a_path_is_found_on_its_line() {
    let source = "fn a() {\n    let x = icon!(\"mdi:home\");\n    telar_icons::icon![\"lucide:search\"];\n}\n";
    assert_eq!(
        ids(source),
        [
            ("mdi:home".to_string(), 2),
            ("lucide:search".to_string(), 3)
        ]
    );
}

#[test]
fn a_call_nested_in_a_builder_is_found() {
    let source = "icon_button(IconButtonProps::props()\n    .icon(telar_icons::icon!(\"lucide:x\"))\n    .build())";
    assert_eq!(ids(source), [("lucide:x".to_string(), 2)]);
}

#[test]
fn a_raw_string_names_its_id() {
    assert_eq!(
        ids("icon!(r\"mdi:home\"); icon!(r#\"mdi:gear\"#)"),
        [("mdi:home".to_string(), 1), ("mdi:gear".to_string(), 1)]
    );
}

#[test]
fn comments_strings_and_characters_name_nothing() {
    let source = r##"
// icon!("a:line")
/* icon!("a:block") /* nested */ icon!("a:after-nested") */
/// icon!("a:doc")
let s = "icon!(\"a:string\")";
let r = r#"icon!("a:raw")"#;
let q = '"'; let e = '\''; let l: &'static str = "x";
let b = b'"';
icon!("a:real");
"##;
    assert_eq!(ids(source), [("a:real".to_string(), 9)]);
}

#[test]
fn a_lifetime_does_not_open_a_character() {
    let source = "fn f<'a>(x: &'a str) { icon!(\"a:after\") }";
    assert_eq!(ids(source), [("a:after".to_string(), 1)]);
}

#[test]
fn another_macro_and_a_call_that_is_not_one_literal_name_nothing() {
    let source = "iconic!(\"a:b\"); icon(\"a:c\"); icon!(ID); icon!(\"a:\\u{64}\"); icon!(b\"a:e\"); icon!(\"a:f\", 1);";
    assert!(ids(source).is_empty(), "{:?}", ids(source));
}

#[test]
fn lines_count_from_where_the_source_starts() {
    let mut out = Vec::new();
    collect_macro_ids(
        "\n\nicon!(\"a:b\")",
        Path::new("view.rsx"),
        10,
        &[icon_kind()],
        &mut out,
    );
    assert_eq!(out[0].line, 12);
    assert_eq!(out[0].written, "\"a:b\"");
}

#[test]
fn a_multiline_string_counts_its_lines() {
    let source = "let s = \"one\ntwo\";\nicon!(\"a:b\")";
    assert_eq!(ids(source), [("a:b".to_string(), 3)]);
}

#[test]
fn source_that_does_not_mention_the_tag_is_not_lexed() {
    assert!(ids("fn main() { println!(\"hi\") }").is_empty());
}
