use super::*;

fn idents(source: &str) -> Vec<String> {
    tokens(source)
        .iter()
        .filter_map(|t| t.ident().map(str::to_owned))
        .collect()
}

fn strings(source: &str) -> Vec<String> {
    tokens(source)
        .iter()
        .filter_map(|t| t.string().map(str::to_owned))
        .collect()
}

#[test]
fn line_and_doc_comments_yield_nothing() {
    assert_eq!(idents("a // b\n/// c\n//! d\ne"), ["a", "e"]);
}

#[test]
fn block_comments_nest_and_end_at_their_own_close() {
    assert_eq!(idents("a /* b /* c */ d */ e /** f */ g"), ["a", "e", "g"]);
}

#[test]
fn a_string_hides_what_looks_like_code_and_keeps_its_escapes_decoded() {
    assert_eq!(idents(r#"a "mod foo; // x" b"#), ["a", "b"]);
    assert_eq!(strings(r#""say \"hi\" \\ done""#), [r#"say "hi" \ done"#]);
    assert_eq!(strings("\"one \\\n    two\""), ["one two"]);
}

#[test]
fn raw_strings_of_any_hash_count_end_only_at_a_matching_close() {
    assert_eq!(strings(r##"r#"a "quoted" b"#"##), [r#"a "quoted" b"#]);
    assert_eq!(strings(r####"r###"x "## y"###"####), [r###"x "## y"###]);
    assert_eq!(strings(r#"r"plain\n""#), [r"plain\n"]);
    assert_eq!(idents(r##"a r#"mod"# b"##), ["a", "b"]);
}

#[test]
fn byte_and_c_strings_are_strings() {
    assert_eq!(
        strings(r##"b"ab" c"cd" br"ef" cr#"gh"#"##),
        ["ab", "cd", "ef", "gh"]
    );
    assert!(idents(r#"b"ab" c"cd" br"ef""#).is_empty());
}

#[test]
fn a_raw_identifier_is_its_name_but_not_the_keyword() {
    let found = tokens("r#type r#mod");
    assert_eq!(idents("r#type"), ["type"]);
    assert!(!found[1].is_word("mod"));
    assert_eq!(found[1].ident(), Some("mod"));
}

#[test]
fn character_literals_are_dropped_and_lifetimes_are_not_mistaken_for_them() {
    assert_eq!(
        idents(r#"a 'a' b '\n' c '\'' d b'x' e"#),
        ["a", "b", "c", "d", "e"]
    );
    assert_eq!(idents("f<'a, 'static> x"), ["f", "x"]);
    assert_eq!(idents("'\"' a"), ["a"]);
    assert_eq!(idents("&'a str 'b: loop {}"), ["str", "loop"]);
}

#[test]
fn numbers_do_not_leave_identifier_fragments() {
    assert_eq!(idents("1u8 0xff_u32 1e5 a"), ["a"]);
}

#[test]
fn depth_counts_the_braces_open_around_a_token() {
    let found = tokens("a { b { c } d } e");
    let depths: Vec<(String, usize)> = found
        .iter()
        .filter_map(|t| Some((t.ident()?.to_owned(), t.depth)))
        .collect();
    assert_eq!(
        depths,
        [
            ("a".into(), 0),
            ("b".into(), 1),
            ("c".into(), 2),
            ("d".into(), 1),
            ("e".into(), 0)
        ]
    );
}

#[test]
fn unterminated_input_ends_quietly() {
    assert_eq!(idents("a /* b"), ["a"]);
    assert_eq!(idents("a \"b"), ["a"]);
    assert_eq!(idents("a r#\"b"), ["a"]);
    assert_eq!(idents("a '\\"), ["a"]);
    assert_eq!(idents("a } }"), ["a"]);
}
