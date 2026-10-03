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
