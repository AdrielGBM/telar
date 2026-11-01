use super::*;

#[test]
fn an_identifier_is_letters_digits_and_underscores_not_led_by_a_digit() {
    for name in ["battery", "_hidden", "cpu2", "ÿes", "snake_case", "π"] {
        assert!(is_identifier(name), "{name}");
    }
    for name in [
        "",
        "2fast",
        "with-dash",
        "with space",
        "dotted.path",
        "$name",
        "ok!",
    ] {
        assert!(!is_identifier(name), "{name}");
    }
}

#[test]
fn every_identifier_lexes_as_one_name_and_one_reference() {
    for name in ["battery", "_hidden", "cpu2", "ÿes"] {
        let tokens = lex(name).unwrap();
        assert!(
            matches!(tokens.as_slice(), [Token { kind: TokenKind::Ident(ident), .. }] if ident == name),
            "{tokens:?}"
        );
        let source = format!("${name}");
        let tokens = lex(&source).unwrap();
        assert!(
            matches!(tokens.as_slice(), [Token { kind: TokenKind::Reference(reference), .. }] if reference.name == name && reference.path.is_empty()),
            "{tokens:?}"
        );
    }
}

#[test]
fn references_are_found_where_they_are_written_and_never_inside_a_text() {
    let source = "fmt('$5 {}', $battery.level) + $label";
    let found = references_in(source).unwrap();
    let written: Vec<(&str, String)> = found
        .iter()
        .map(|(span, reference)| (&source[span.range()], reference.dotted()))
        .collect();
    assert_eq!(
        written,
        [
            ("$battery.level", "battery.level".to_string()),
            ("$label", "label".to_string())
        ]
    );
    assert!(references_in("'unclosed").is_err());
}
