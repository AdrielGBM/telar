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
        host_attributes: vec![
            ("data-telar".to_string(), String::new()),
            ("style".to_string(), "position:relative;".to_string()),
        ],
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

#[test]
fn a_page_takes_its_language_title_markup_and_state_from_what_the_app_wrote() {
    let html = page_for(&base(), &written(Vec::new(), &[]), Some(2))
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
        html.contains(r#"import init from "../../app-0123456789ab.js";"#),
        "{html}"
    );
}

#[test]
fn the_page_served_at_any_address_writes_its_urls_from_the_root() {
    let page = page_for(&base(), &written(Vec::new(), &[]), None);
    assert_eq!(page.base, "/");
    assert_eq!(
        page_for(&base(), &written(Vec::new(), &[]), Some(0)).base,
        "./"
    );
}

#[test]
fn the_binary_is_the_one_cargo_built_for_this_package() {
    let manifest = std::env::temp_dir().join(format!(
        "telar-prerender-binary-{}/Cargo.toml",
        std::process::id()
    ));
    std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
    std::fs::write(&manifest, "[package]\nname = \"site\"\n").unwrap();
    let artifact = |manifest: &Path, name: &str, executable: Option<&str>| {
        serde_json::json!({
            "reason": "compiler-artifact",
            "manifest_path": manifest,
            "target": { "name": name, "kind": ["bin"] },
            "executable": executable,
        })
        .to_string()
    };
    let messages = [
        artifact(
            Path::new("/elsewhere/Cargo.toml"),
            "other",
            Some("/t/other"),
        ),
        artifact(&manifest, "site", None),
        artifact(&manifest, "tool", Some("/t/tool")),
        artifact(&manifest, "site", Some("/t/site")),
        "not json".to_string(),
    ]
    .join("\n");
    assert_eq!(
        executable_in(&messages, &manifest),
        Some(PathBuf::from("/t/site"))
    );
    let _ = std::fs::remove_dir_all(manifest.parent().unwrap());
}
