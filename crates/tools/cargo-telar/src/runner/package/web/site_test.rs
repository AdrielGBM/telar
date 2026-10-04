use std::path::PathBuf;

use super::*;

fn web(toml: &str) -> WebSection {
    toml::from_str(toml).expect("a web table parses")
}

fn site(toml: &str) -> Site {
    Site::new(&web(toml), Path::new("/nonexistent"), "demo").unwrap()
}

fn at(segments: &[&str], locale: Option<&str>) -> PageLocation {
    PageLocation {
        segments: segments.iter().map(|segment| segment.to_string()).collect(),
        locale: locale.map(str::to_string),
    }
}

fn localized() -> SitePages {
    SitePages {
        pages: vec![
            at(&[], Some("es")),
            at(&["projects"], Some("es")),
            at(&[], Some("en")),
            at(&["projects"], Some("en")),
        ],
        locales: vec!["es".to_string(), "en".to_string()],
        base_locale: Some("es".to_string()),
        not_found: true,
    }
}

fn html(tags: &[HeadTag]) -> String {
    tags.iter()
        .map(HeadTag::html)
        .collect::<Vec<_>>()
        .join("\n")
}

/// A package root holding `files`, each a path and its content.
fn package(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_site_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, content) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content).unwrap();
    }
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn a_page_is_served_at_its_directory_under_the_base() {
    let root = site("");
    assert_eq!(root.path_of(&PageLocation::root()), "/");
    assert_eq!(
        root.path_of(&at(&["projects"], Some("es"))),
        "/es/projects/"
    );
    assert_eq!(root.url_of(&PageLocation::root()), None);

    let docs = site("origin = \"https://example.com/\"\nbase = \"docs\"");
    assert_eq!(
        docs.path_of(&at(&["a b", "ñ"], None)),
        "/docs/a%20b/%C3%B1/"
    );
    assert_eq!(
        docs.url_of(&at(&[], Some("en"))).as_deref(),
        Some("https://example.com/docs/en/")
    );
}

#[test]
fn a_site_that_says_nothing_describes_itself_as_the_app() {
    let tags = html(&site("").head_tags(&PageMeta {
        title: "demo",
        lang: "en",
        location: Some(&PageLocation::root()),
        pages: &SitePages::root_only(),
    }));
    assert_eq!(
        tags,
        [
            r#"<meta name="description" content="demo, a Telar application." />"#,
            r#"<meta property="og:type" content="website" />"#,
            r#"<meta property="og:title" content="demo" />"#,
            r#"<meta property="og:description" content="demo, a Telar application." />"#,
            r#"<meta property="og:locale" content="en" />"#,
            r#"<meta name="twitter:card" content="summary" />"#,
        ]
        .join("\n")
    );
}

#[test]
fn a_page_in_a_locale_links_its_translations_and_previews_its_own_picture() {
    let root = package("preview", &[("web/public/og/es.png", "png")]);
    let site = Site::new(
        &web("origin = \"https://example.com\"\nog_image = { es = \"og/es.png\", en = \"https://cdn.example.com/en.png\" }\ntheme_color = { light = \"#fff\", dark = \"#000\" }"),
        &root,
        "demo",
    )
    .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    let pages = localized();
    let projects = at(&["projects"], Some("en"));
    let tags = html(&site.head_tags(&PageMeta {
        title: "Projects",
        lang: "en",
        location: Some(&projects),
        pages: &pages,
    }));
    for expected in [
        r##"<meta name="theme-color" content="#fff" media="(prefers-color-scheme: light)" />"##,
        r##"<meta name="theme-color" content="#000" media="(prefers-color-scheme: dark)" />"##,
        r#"<link rel="canonical" href="https://example.com/en/projects/" />"#,
        r#"<link rel="alternate" href="https://example.com/es/projects/" hreflang="es" />"#,
        r#"<link rel="alternate" href="https://example.com/en/projects/" hreflang="en" />"#,
        r#"<link rel="alternate" href="https://example.com/es/projects/" hreflang="x-default" />"#,
        r#"<meta property="og:url" content="https://example.com/en/projects/" />"#,
        r#"<meta property="og:locale" content="en" />"#,
        r#"<meta property="og:locale:alternate" content="es" />"#,
        r#"<meta property="og:image" content="https://cdn.example.com/en.png" />"#,
        r#"<meta name="twitter:card" content="summary_large_image" />"#,
    ] {
        assert!(tags.contains(expected), "{expected} in\n{tags}");
    }

    let tags = html(&site.head_tags(&PageMeta {
        title: "Inicio",
        lang: "es-CL",
        location: Some(&at(&[], Some("es"))),
        pages: &pages,
    }));
    assert!(
        tags.contains(r#"<meta property="og:image" content="https://example.com/og/es.png" />"#),
        "{tags}"
    );
    assert!(
        tags.contains(r#"<meta property="og:locale" content="es_CL" />"#),
        "{tags}"
    );
}

#[test]
fn a_site_served_under_a_path_says_so_first() {
    let tags = site("base = \"/docs/\"").head_tags(&PageMeta {
        title: "demo",
        lang: "en",
        location: None,
        pages: &SitePages::root_only(),
    });
    assert_eq!(tags[0].html(), r#"<base href="/docs/" />"#);
    assert!(!html(&tags).contains("canonical"));
}

#[test]
fn the_description_is_read_from_the_catalog_in_the_page_locale() {
    let root = package(
        "description",
        &[
            ("telar.toml", "[telar.i18n]\ndefault = \"es\"\n"),
            (
                "locales/es.toml",
                "[meta]\ndescription = \"Un portafolio\"\n",
            ),
            ("locales/en.toml", "[meta]\ndescription = \"A portfolio\"\n"),
            ("locales/fr.toml", "[meta]\nother = \"Autre\"\n"),
        ],
    );
    let site = Site::new(&web("description = \"meta.description\""), &root, "demo").unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(site.description("en"), "A portfolio");
    assert_eq!(site.description("en-GB"), "A portfolio");
    assert_eq!(site.description("es"), "Un portafolio");
    assert_eq!(site.description("fr"), "Un portafolio");
}

#[test]
fn a_description_the_catalog_cannot_give_as_plain_text_is_an_error() {
    let root = package(
        "description_errors",
        &[(
            "locales/en.toml",
            "greeting = \"Hello {name}\"\n[items]\none = \"An item\"\nother = \"{count} items\"\n",
        )],
    );
    for (key, says) in [
        ("missing", "not a key"),
        ("greeting", "plain text"),
        ("items", "plain text"),
    ] {
        let error =
            Site::new(&web(&format!("description = \"{key}\"")), &root, "demo").unwrap_err();
        assert!(error.contains(says), "{key}: {error}");
    }
    let _ = std::fs::remove_dir_all(&root);

    let empty = package("description_no_catalog", &[]);
    let error = Site::new(&web("description = \"meta\""), &empty, "demo").unwrap_err();
    let _ = std::fs::remove_dir_all(&empty);
    assert!(error.contains("no catalog"), "{error}");
}

#[test]
fn a_preview_picture_of_the_site_has_to_be_in_the_public_directory() {
    let root = package("og_image", &[("web/public/og/es.png", "png")]);
    let site = |image: &str| {
        Site::new(
            &web(&format!(
                "origin = \"https://example.com\"\nog_image = {image}"
            )),
            &root,
            "demo",
        )
    };
    assert!(site("\"og/es.png\"").is_ok());
    assert!(site("\"https://cdn.example.com/x.png\"").is_ok());
    let error = site("{ es = \"og/es.png\", en = \"og/en.png\" }").unwrap_err();
    let _ = std::fs::remove_dir_all(&root);
    assert!(error.contains("og/en.png"), "{error}");
}

#[test]
fn the_sitemap_lists_every_page_with_its_translations() {
    let sitemap = site("origin = \"https://example.com\"")
        .sitemap(&localized())
        .unwrap();
    assert!(sitemap.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">\n"));
    assert_eq!(sitemap.matches("<url>").count(), 4);
    assert!(sitemap.contains(
        "  <url>\n    <loc>https://example.com/en/projects/</loc>\n    <xhtml:link rel=\"alternate\" hreflang=\"es\" href=\"https://example.com/es/projects/\"/>\n    <xhtml:link rel=\"alternate\" hreflang=\"en\" href=\"https://example.com/en/projects/\"/>\n    <xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"https://example.com/es/projects/\"/>\n  </url>\n"
    ), "{sitemap}");
    assert!(sitemap.ends_with("</urlset>\n"));
}

#[test]
fn a_site_with_no_origin_writes_no_sitemap() {
    assert_eq!(site("").sitemap(&localized()), None);
    let plain = site("origin = \"https://example.com\"")
        .sitemap(&SitePages::root_only())
        .unwrap();
    assert!(
        plain.contains("<loc>https://example.com/</loc>\n  </url>"),
        "{plain}"
    );
}
