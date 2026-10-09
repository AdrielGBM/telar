use super::*;

fn spans(line: &str) -> Vec<(TokenKind, &str)> {
    header_tokens(line)
        .into_iter()
        .map(|token| (token.kind, &line[token.start..token.start + token.len]))
        .collect()
}

fn place(prefix: &str) -> Place {
    place_at_end(prefix).expect("a preview header").1
}

#[test]
fn tells_a_variant_from_the_meta_section() {
    assert_eq!(
        header_kind("[preview \"A\"]").map(|h| h.0),
        Some(HeaderKind::Variant)
    );
    assert_eq!(
        header_kind("[previews \"Forms/Checkbox\"]").map(|h| h.0),
        Some(HeaderKind::Meta)
    );
    assert_eq!(header_kind("[previewish]"), None);
    assert_eq!(header_kind("[preview, next]"), None, "Rust, not a header");
    assert_eq!(
        header_kind("[preview]").map(|h| h.0),
        Some(HeaderKind::Variant)
    );
    assert_eq!(header_kind("[view]"), None);
    assert_eq!(header_kind("col"), None);
}

#[test]
fn names_option_keys_arg_names_and_matrix_axes() {
    let line = "[preview \"Bound\" args(agree:false label:\"a b\") layout:centered matrix:(dir:[ltr rtl] size:[12 16]) animate]";
    assert_eq!(
        spans(line),
        vec![
            (TokenKind::OptionKey, "args"),
            (TokenKind::ArgName, "agree"),
            (TokenKind::ArgName, "label"),
            (TokenKind::OptionKey, "layout"),
            (TokenKind::OptionKey, "matrix"),
            (TokenKind::MatrixAxis, "dir"),
            (TokenKind::MatrixAxis, "size"),
            (TokenKind::OptionKey, "animate"),
        ]
    );
}

#[test]
fn a_name_holding_what_looks_like_options_is_only_a_name() {
    let line = "[preview \"layout:centered args(x:1)\" tags:[a b]]";
    assert_eq!(spans(line), vec![(TokenKind::OptionKey, "tags")]);
}

#[test]
fn finds_where_the_cursor_falls_in_a_header_being_typed() {
    assert_eq!(place("[preview \"A\" "), Place::OptionKey);
    assert_eq!(place("[preview \"A\" lay"), Place::OptionKey);
    assert_eq!(
        place("[preview \"A\" layout:"),
        Place::OptionValue("layout".into())
    );
    assert_eq!(
        place("[previews \"T\" matrix:th"),
        Place::OptionValue("matrix".into())
    );
    assert_eq!(place("[preview \"A\" args("), Place::ArgName);
    assert_eq!(place("[preview \"A\" args(agree:"), Place::ArgValue);
    assert_eq!(place("[preview \"A\" args(agree:false "), Place::ArgName);
    assert_eq!(place("[preview \"A\" matrix:("), Place::MatrixAxis);
    assert_eq!(
        place("[preview \"A\" matrix:(dir:[ltr "),
        Place::MatrixAxisValue("dir".into())
    );
    assert_eq!(
        place("[preview \"A\" args(a:1) "),
        Place::OptionKey,
        "past the args"
    );
}

#[test]
fn no_completion_place_inside_a_string_or_past_the_header() {
    assert_eq!(place("[preview \"Def"), Place::Elsewhere);
    assert_eq!(place("[preview "), Place::Elsewhere);
    assert_eq!(place("[preview \"A\""), Place::Elsewhere);
    assert_eq!(place("[preview \"A\"] "), Place::Elsewhere);
    assert_eq!(place("[preview \"A\" args(label:\"x y"), Place::Elsewhere);
}

#[test]
fn finds_the_token_under_a_cursor() {
    let line = "[preview \"A\" layout:centered]";
    let (token, text) = token_at(line, line.find("layout").unwrap() + 2).unwrap();
    assert_eq!((token.kind, text), (TokenKind::OptionKey, "layout"));
    assert!(token_at(line, line.find("centered").unwrap() + 2).is_none());
}

#[test]
fn every_option_the_transpiler_reads_is_described() {
    let described: Vec<&str> = options().map(|option| option.key).collect();
    assert_eq!(described, telar_transpiler::PREVIEW_OPTION_KEYS);
    for option in options() {
        assert!(!option.doc.is_empty(), "{}", option.key);
        assert!(option.snippet.starts_with(option.key), "{}", option.key);
    }
}
