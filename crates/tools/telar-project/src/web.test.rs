use super::*;

fn web(toml: &str) -> WebSection {
    toml::from_str(toml).expect("a web table parses")
}

#[test]
fn the_site_is_at_the_root_unless_a_base_says_otherwise() {
    assert_eq!(web("").base_path(), "/");
    assert_eq!(web("base = \"/\"").base_path(), "/");
    for spelling in ["docs", "/docs", "docs/", "/docs/"] {
        assert_eq!(web(&format!("base = \"{spelling}\"")).base_path(), "/docs/");
    }
    assert_eq!(web("base = \"/a/b\"").base_path(), "/a/b/");
}

#[test]
fn an_origin_is_written_without_its_closing_slash() {
    assert_eq!(
        web("origin = \"https://example.com/\"").origin(),
        Some("https://example.com")
    );
    assert_eq!(web("").origin(), None);
}

#[test]
fn an_origin_is_a_scheme_and_a_host_and_nothing_more() {
    for good in [
        "https://example.com",
        "http://localhost:8080",
        "https://a.b/",
    ] {
        assert!(
            web(&format!("origin = \"{good}\"")).problems().is_empty(),
            "{good}"
        );
    }
    for bad in [
        "example.com",
        "https://",
        "https://example.com/docs",
        "https://example.com?x",
        "ftp://example.com",
    ] {
        let problems = web(&format!("origin = \"{bad}\"")).problems();
        assert!(
            problems.len() == 1 && problems[0].contains("origin"),
            "{bad}: {problems:?}"
        );
    }
}

#[test]
fn a_base_is_a_plain_path() {
    for bad in ["/a/../b", "/a//b", "/a b/", "/a?x", "/a#x", "/a%20b"] {
        let problems = web(&format!("base = \"{bad}\"")).problems();
        assert!(
            problems.len() == 1 && problems[0].contains("base"),
            "{bad}: {problems:?}"
        );
    }
}

#[test]
fn a_link_preview_picture_of_the_site_needs_its_origin() {
    let problems = web("og_image = \"og.png\"").problems();
    assert!(
        problems.len() == 1 && problems[0].contains("origin"),
        "{problems:?}"
    );
    assert!(
        web("og_image = \"https://cdn.example.com/og.png\"")
            .problems()
            .is_empty()
    );
    assert!(
        web("origin = \"https://example.com\"\nog_image = \"og.png\"")
            .problems()
            .is_empty()
    );
}

#[test]
fn a_picture_per_locale_falls_back_to_the_base_locale() {
    let image = web("og_image = { es = \"og/es.png\", en = \"og/en.png\" }")
        .og_image
        .unwrap();
    assert_eq!(image.for_locale("en", "es"), Some("og/en.png"));
    assert_eq!(image.for_locale("fr", "es"), Some("og/es.png"));
    assert_eq!(
        OgImage::Shared("og.png".to_string()).for_locale("fr", "es"),
        Some("og.png")
    );
}

#[test]
fn a_theme_color_is_one_or_one_per_scheme() {
    assert_eq!(
        web("theme_color = \"#101010\"").theme_color,
        Some(ThemeColor::One("#101010".to_string()))
    );
    assert_eq!(
        web("theme_color = { light = \"#fff\", dark = \"#000\" }").theme_color,
        Some(ThemeColor::Schemes(SchemeColors {
            light: "#fff".to_string(),
            dark: "#000".to_string(),
        }))
    );
    assert!(toml::from_str::<WebSection>("theme_color = { light = \"#fff\" }").is_err());
}

#[test]
fn the_host_is_static_unless_named() {
    assert_eq!(web("").host(), WebHost::Static);
    assert_eq!(
        web("host = \"cloudflare-pages\"").host(),
        WebHost::CloudflarePages
    );
    assert!(toml::from_str::<WebSection>("host = \"netlify\"").is_err());
}

#[test]
fn cloudflare_pages_serves_the_site_at_the_root() {
    let problems = web("host = \"cloudflare-pages\"\nbase = \"/docs/\"").problems();
    assert!(
        problems.len() == 1 && problems[0].contains("cloudflare-pages"),
        "{problems:?}"
    );
}
