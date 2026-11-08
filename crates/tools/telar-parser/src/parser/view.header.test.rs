use super::*;

fn for_header(rest: &str) -> ForHeader {
    split_for_in(rest).unwrap_or_else(|why| panic!("{why}"))
}

#[test]
fn a_for_iterable_keeps_the_spacing_of_its_string_literals() {
    let header = for_header(r#"word in "a  b   c".split(' ')"#);
    assert_eq!(header.pattern, "word");
    assert_eq!(header.iterable, r#""a  b   c".split(' ')"#);
}

#[test]
fn keywords_inside_a_string_literal_are_not_for_keywords() {
    let header = for_header(r#"x in names("a in b virtual gap:3 key k") key x.id"#);
    assert_eq!(header.iterable, r#"names("a in b virtual gap:3 key k")"#);
    assert_eq!(header.key_expr.as_deref(), Some("x.id"));
    assert_eq!(header.gap_expr, None);
    assert!(header.virtual_clause.is_none());
}

#[test]
fn a_gap_value_may_be_a_string_literal_with_spaces() {
    let header = for_header(r#"x in xs gap:pick("a  b")"#);
    assert_eq!(header.iterable, "xs");
    assert_eq!(header.gap_expr.as_deref(), Some(r#"pick("a  b")"#));
}

#[test]
fn an_escaped_quote_does_not_end_a_string_literal() {
    let header = for_header(r#"x in xs.filter("say \" in  \" now") key x"#);
    assert_eq!(header.iterable, r#"xs.filter("say \" in  \" now")"#);
    assert_eq!(header.key_expr.as_deref(), Some("x"));
}

#[test]
fn a_raw_string_literal_is_one_token_whatever_it_holds() {
    let header = for_header(r##"x in lines(r#"a " in  b"#) key x"##);
    assert_eq!(header.iterable, r##"lines(r#"a " in  b"#)"##);
    assert_eq!(header.key_expr.as_deref(), Some("x"));

    let header = for_header(r#"x in lines(br"in  key") gap:2"#);
    assert_eq!(header.iterable, r#"lines(br"in  key")"#);
    assert_eq!(header.gap_expr.as_deref(), Some("2"));
}

#[test]
fn a_char_literal_hides_the_quote_it_holds() {
    let header = for_header(r#"c in text.split('"').chain("x  y".chars()) key c"#);
    assert_eq!(header.iterable, r#"text.split('"').chain("x  y".chars())"#);
    assert_eq!(header.key_expr.as_deref(), Some("c"));

    let header = for_header(r"c in text.split('\'').filter(f)  key c");
    assert_eq!(header.iterable, r"text.split('\'').filter(f)");
    assert_eq!(header.key_expr.as_deref(), Some("c"));
}

#[test]
fn a_lifetime_is_not_taken_for_a_char_literal() {
    let header = for_header("x in items::<&'a str>()  key x");
    assert_eq!(header.iterable, "items::<&'a str>()");
    assert_eq!(header.key_expr.as_deref(), Some("x"));
}

#[test]
fn a_for_header_still_splits_on_its_own_keywords() {
    let header = for_header("(i,  item) in $items key item.id gap:8");
    assert_eq!(header.pattern, "(i,  item)");
    assert_eq!(header.iterable, "$items");
    assert_eq!(header.key_expr.as_deref(), Some("item.id"));
    assert_eq!(header.gap_expr.as_deref(), Some("8"));

    let header = for_header("row in $rows virtual row_height:32");
    assert_eq!(header.iterable, "$rows");
    assert_eq!(
        header.virtual_clause.and_then(|clause| clause.row_height),
        Some("32".to_string())
    );
}

#[test]
fn a_for_header_without_an_iterable_is_refused() {
    assert!(split_for_in(r#"x in"#).is_err());
    assert!(split_for_in(r#""in" x"#).is_err());
}

#[test]
fn a_match_scrutinee_keeps_the_spacing_of_its_string_literals() {
    let (scrutinee, binding, key) = split_match_header(r#"lookup("a  b")  as v"#).unwrap();
    assert_eq!(scrutinee, r#"lookup("a  b")"#);
    assert_eq!(binding.as_deref(), Some("v"));
    assert_eq!(key, None);
}

#[test]
fn keywords_inside_a_match_literal_are_not_keywords() {
    let (scrutinee, binding, key) =
        split_match_header(r#"f("x as y key z") key r"k  as q""#).unwrap();
    assert_eq!(scrutinee, r#"f("x as y key z")"#);
    assert_eq!(binding, None);
    assert_eq!(key.as_deref(), Some(r#"r"k  as q""#));

    let (scrutinee, binding, key) = split_match_header(r"c.eq('\'') as flag key  flag").unwrap();
    assert_eq!(scrutinee, r"c.eq('\'')");
    assert_eq!(binding.as_deref(), Some("flag"));
    assert_eq!(key.as_deref(), Some("flag"));
}

#[test]
fn an_empty_match_header_is_refused() {
    assert!(split_match_header("   ").is_none());
}
