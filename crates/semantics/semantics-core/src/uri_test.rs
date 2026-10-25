use super::*;

#[test]
fn a_uri_needs_a_scheme_and_something_after_it() {
    assert!(Uri::parse("https://example.com").is_ok());
    assert!(Uri::parse("mailto:someone@example.com").is_ok());
    assert_eq!(Uri::parse("/projects/telar"), Err(UriError::NotAbsolute));
    assert_eq!(Uri::parse("projects"), Err(UriError::NotAbsolute));
    assert_eq!(Uri::parse("1http://x"), Err(UriError::NotAbsolute));
    assert_eq!(Uri::parse("https:"), Err(UriError::NotAbsolute));
    assert_eq!(Uri::parse(""), Err(UriError::NotAbsolute));
}

#[test]
fn the_default_schemes_need_no_registration() {
    for uri in [
        "http://example.com",
        "https://example.com",
        "mailto:someone@example.com",
        "tel:+34600000000",
        "sms:+34600000000",
    ] {
        assert!(Uri::parse(uri).is_ok(), "{uri}");
    }
}

#[test]
fn the_scheme_says_whether_it_is_a_web_page() {
    let page = Uri::parse("HTTPS://example.com").unwrap();
    assert_eq!(page.scheme(), "HTTPS");
    assert!(page.is_web());
    assert!(!Uri::parse("mailto:a@b.c").unwrap().is_web());
}

#[test]
fn script_data_and_file_uris_are_refused_even_once_registered() {
    for scheme in ["javascript", "vbscript", "data", "file", "JavaScript", "c"] {
        assert_eq!(
            allow_uri_scheme(scheme),
            Err(UriError::DeniedScheme(scheme.to_ascii_lowercase().into())),
            "{scheme}"
        );
    }
    for (uri, scheme) in [
        ("javascript:alert(1)", "javascript"),
        ("JaVaScRiPt:alert(1)", "javascript"),
        ("vbscript:msgbox(1)", "vbscript"),
        ("data:text/html,<script>alert(1)</script>", "data"),
        ("file:///etc/passwd", "file"),
        ("C:\\x.exe", "c"),
    ] {
        assert_eq!(
            Uri::parse(uri),
            Err(UriError::DeniedScheme(scheme.into())),
            "{uri}"
        );
    }
}

#[test]
fn a_single_letter_scheme_is_a_drive_path() {
    assert_eq!(
        Uri::parse("d:/work/app.exe"),
        Err(UriError::DeniedScheme("d".into()))
    );
    assert!(
        UriError::DeniedScheme("d".into())
            .to_string()
            .contains("drive path")
    );
}

#[test]
fn an_app_scheme_opens_only_once_the_app_allows_it() {
    assert_eq!(
        Uri::parse("telar-test-app://settings"),
        Err(UriError::SchemeNotAllowed("telar-test-app".into()))
    );
    allow_uri_scheme("telar-test-app").unwrap();
    assert_eq!(
        Uri::parse("telar-test-app://settings").unwrap().as_str(),
        "telar-test-app://settings"
    );
}

#[test]
fn schemes_match_ignoring_case() {
    allow_uri_scheme("Telar-Case-Test").unwrap();
    assert!(Uri::parse("telar-case-test:x").is_ok());
    assert!(Uri::parse("TELAR-CASE-TEST:x").is_ok());
    assert!(is_uri_scheme_allowed("HtTpS"));
    assert!(!is_uri_scheme_allowed("FILE"));
}

#[test]
fn whitespace_and_control_characters_are_refused_anywhere() {
    for (uri, found) in [
        ("java\tscript:alert(1)", '\t'),
        ("https://example.com/a b", ' '),
        (" https://example.com", ' '),
        ("https://example.com\n", '\n'),
        ("https://example.com/\u{1b}]8;;", '\u{1b}'),
        ("https://example.com/\u{0}", '\u{0}'),
        ("https://example.com/\u{85}", '\u{85}'),
        ("https://example.com/\u{3000}", '\u{3000}'),
    ] {
        assert_eq!(
            Uri::parse(uri),
            Err(UriError::ForbiddenCharacter(found)),
            "{uri:?}"
        );
    }
}

#[test]
fn only_a_scheme_can_be_allowed() {
    for text in ["", "my app", "1app", "myapp:", "my/app"] {
        assert_eq!(
            allow_uri_scheme(text),
            Err(UriError::InvalidScheme(text.into())),
            "{text:?}"
        );
    }
}

#[test]
fn a_uri_reads_from_a_str() {
    assert_eq!(
        "https://example.com".parse::<Uri>(),
        Uri::parse("https://example.com")
    );
}

#[test]
fn a_uri_must_be_ascii() {
    for (uri, found) in [
        ("https://example.com/\u{202e}evil", '\u{202e}'),
        ("https://exam\u{200b}ple.com", '\u{200b}'),
        ("https://bücher.example", 'ü'),
        ("https://example.com/\u{feff}", '\u{feff}'),
    ] {
        assert_eq!(Uri::parse(uri), Err(UriError::NotAscii(found)), "{uri:?}");
    }
    assert!(Uri::parse("https://b%C3%BCcher.example").is_ok());
    assert!(
        UriError::NotAscii('ü')
            .to_string()
            .contains("percent-encode")
    );
}

#[test]
fn surrounding_whitespace_is_refused_not_trimmed() {
    for uri in [
        " https://example.com",
        "https://example.com ",
        "\thttps://example.com",
    ] {
        assert!(matches!(
            Uri::parse(uri),
            Err(UriError::ForbiddenCharacter(_))
        ));
    }
}

#[test]
fn a_route_address_stays_inside_the_app() {
    for address in [
        "/projects/telar",
        "/",
        "",
        "projects",
        "/a?next=https://x.example",
        "/a#b:c",
        "/a:b",
    ] {
        assert_eq!(RouteAddress::parse(address).unwrap().as_str(), address);
    }
    for address in [
        "https://example.com",
        "mailto:someone@example.com",
        "javascript:alert(1)",
        "my-app:x",
        "C:\\x.exe",
        "//example.com",
        "/\\example.com",
        "\\\\example.com",
    ] {
        assert_eq!(
            RouteAddress::parse(address),
            Err(UriError::NotRouteAddress),
            "{address:?}"
        );
    }
    assert_eq!(
        RouteAddress::parse("/a\u{202e}"),
        Err(UriError::NotAscii('\u{202e}'))
    );
    assert_eq!(
        RouteAddress::parse("/a b"),
        Err(UriError::ForbiddenCharacter(' '))
    );
}
