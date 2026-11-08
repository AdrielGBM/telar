use std::collections::BTreeMap;

use telar_project::{Preferences, PrerenderState, STATE_VERSION, Surface};

use super::super::page::{Bootstrap, DEFAULT_TEMPLATE};
use super::*;

fn at(segments: &[&str]) -> PageLocation {
    PageLocation {
        segments: segments.iter().map(|segment| segment.to_string()).collect(),
        locale: None,
    }
}

fn written(pages: Vec<PageLocation>, locales: &[&str]) -> PrerenderedPage {
    PrerenderedPage {
        lang: Some("es".to_string()),
        title: "Proyectos — Portafolio".to_string(),
        host_attributes_html: " data-telar=\"\" style=\"position:relative;\"".to_string(),
        head: "<style id=\"telar-reset\"></style>".to_string(),
        markup: "<main data-telar-id=\"4\">Hola</main>".to_string(),
        state: PrerenderState {
            version: STATE_VERSION,
            location: Some("/es/projects".to_string()),
            locale: Some("es".to_string()),
            preferences: Preferences::default(),
            surface: Surface::default(),
            signals: BTreeMap::new(),
        },
        pages,
        locales: locales.iter().map(|locale| locale.to_string()).collect(),
        base_locale: locales.first().map(|locale| locale.to_string()),
        settled: true,
    }
}

fn base() -> Page {
    let mut page = Page::new(
        "portfolio",
        Bootstrap {
            glue: "app-0123456789ab.js".to_string(),
            module: "app_bg-ba9876543210.wasm".to_string(),
        },
    );
    page.lang = "en".to_string();
    page
}

#[test]
fn every_page_is_written_once_and_the_root_always() {
    let pages = plan(&written(vec![at(&["projects"]), at(&["projects"])], &[]));
    assert_eq!(pages, [PageLocation::root(), at(&["projects"])]);
}

#[test]
fn every_page_is_written_in_every_locale_the_address_carries() {
    let pages = plan(&written(vec![at(&["projects"])], &["en", "es"]));
    assert_eq!(
        pages,
        [
            PageLocation::root().in_locale("en"),
            at(&["projects"]).in_locale("en"),
            PageLocation::root().in_locale("es"),
            at(&["projects"]).in_locale("es"),
        ]
    );
}

fn site(toml: &str) -> Site {
    let web: telar_project::WebSection = toml::from_str(toml).unwrap();
    Site::new(&web, Path::new("/nonexistent"), "portfolio").unwrap()
}

#[test]
fn a_page_takes_its_language_title_markup_and_state_from_what_the_app_wrote() {
    let location = at(&["projects"]).in_locale("es");
    let html = page_for(
        &base(),
        &written(Vec::new(), &[]),
        &site(""),
        Some(&location),
        &SitePages::root_only(),
    )
    .render(DEFAULT_TEMPLATE)
    .unwrap();
    assert!(html.contains(r#"<html lang="es" dir="ltr">"#), "{html}");
    assert!(
        html.contains("<title>Proyectos — Portafolio</title>"),
        "{html}"
    );
    assert!(
        html.contains(r#"data-telar-renderer="auto" data-telar="" style="position:relative;"><main data-telar-id="4">Hola</main></div>"#),
        "{html}"
    );
    assert!(
        html.contains("<style id=\"telar-reset\"></style>"),
        "{html}"
    );
    assert!(html.contains(r#""location":"/es/projects""#), "{html}");
    assert!(
        html.contains(r#"import init from "/app-0123456789ab.js";"#),
        "{html}"
    );
}

#[test]
fn a_page_carries_its_description_canonical_and_alternates() {
    let site = site("origin = \"https://example.com\"");
    let pages = site_pages(&written(vec![at(&["projects"])], &["es", "en"]));
    let location = at(&["projects"]).in_locale("en");
    let html = page_for(
        &base(),
        &written(Vec::new(), &[]),
        &site,
        Some(&location),
        &pages,
    )
    .render(DEFAULT_TEMPLATE)
    .unwrap();
    for tag in [
        r#"<link rel="canonical" href="https://example.com/en/projects/" />"#,
        r#"<link rel="alternate" href="https://example.com/es/projects/" hreflang="es" />"#,
        r#"<link rel="alternate" href="https://example.com/es/projects/" hreflang="x-default" />"#,
        r#"<meta property="og:title" content="Proyectos — Portafolio" />"#,
    ] {
        assert!(html.contains(tag), "{tag} in {html}");
    }
}

#[test]
fn the_page_for_a_missing_address_names_no_place_of_its_own() {
    let site = site("origin = \"https://example.com\"\nbase = \"/docs/\"");
    let page = page_for(
        &base(),
        &written(Vec::new(), &[]),
        &site,
        None,
        &SitePages::root_only(),
    );
    let html = page.render(DEFAULT_TEMPLATE).unwrap();
    assert!(!html.contains("canonical"), "{html}");
    assert!(html.contains(r#"<base href="/docs/" />"#), "{html}");
    assert!(
        html.contains(r#"import init from "/docs/app-0123456789ab.js";"#),
        "{html}"
    );
}

#[test]
fn the_pages_of_a_site_whose_addresses_carry_a_locale_fall_back_to_its_base_locale() {
    let mut root = written(vec![at(&["projects"])], &["en", "es"]);
    root.base_locale = Some("es".to_string());
    let pages = site_pages(&root);
    assert_eq!(pages.base_locale.as_deref(), Some("es"));
    assert_eq!(pages.pages.len(), 4);
    assert!(!pages.pages.contains(&PageLocation::root()));

    let plain = site_pages(&written(vec![at(&["projects"]), at(&[".."])], &[]));
    assert_eq!(plain.base_locale, None);
    assert_eq!(plain.pages, [PageLocation::root(), at(&["projects"])]);
}

#[test]
fn the_prerender_host_build_forwards_the_cargo_arguments_verbatim() {
    let passthrough: Vec<String> = ["--package=site", "-r", "--features=analytics"]
        .map(str::to_string)
        .to_vec();
    assert_eq!(
        host_binary_args(&passthrough, &["--no-default-features".to_string()]),
        [
            "build",
            "--bins",
            "--package=site",
            "-r",
            "--features=analytics",
            "--no-default-features",
            "--features",
            "telar/prerender",
            "--message-format=json",
        ]
    );
}
