use std::process::Command;

use super::*;

/// What `script` prints, run by Node.js, which the project's dev shell provides.
fn node(script: &str) -> String {
    let output = Command::new("node")
        .arg("-e")
        .arg(script)
        .output()
        .expect("node runs: the dev shell provides it");
    assert!(
        output.status.success(),
        "node failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn site(toml: &str) -> Site {
    let web: telar_project::WebSection = toml::from_str(toml).unwrap();
    Site::new(&web, std::path::Path::new("/nonexistent"), "demo").unwrap()
}

fn pages() -> SitePages {
    SitePages {
        pages: vec![
            PageLocation::root().in_locale("es"),
            PageLocation::root().in_locale("en"),
        ],
        locales: vec!["es".to_string(), "en".to_string()],
        base_locale: Some("es".to_string()),
        not_found: true,
    }
}

fn titles() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("es".to_string(), "Portafolio".to_string()),
        ("en".to_string(), "Portfolio".to_string()),
    ])
}

#[test]
fn the_page_and_the_app_negotiate_alike() {
    let cases: Vec<(Vec<&str>, Vec<&str>, &str)> = vec![
        (vec!["es-CL", "en"], vec!["en", "es"], "en"),
        (vec!["fr"], vec!["en", "es"], "es"),
        (vec!["PT-br"], vec!["es", "en", "pt-BR"], "es"),
        (vec!["pt"], vec!["es", "en", "pt-BR"], "es"),
        (vec!["es-CL"], vec!["es-ES", "es-MX", "es"], "es"),
        (vec!["en-US", "es"], vec!["es", "en-GB"], "es"),
        (vec![], vec!["es", "en"], "es"),
        (vec!["es"], vec![], "en"),
        (vec!["", "en"], vec!["es", "en"], "es"),
        (vec!["-x", "EN"], vec!["es", "en"], "es"),
        (vec!["zh-Hant-TW"], vec!["zh-Hans", "zh-Hant"], "en"),
    ];
    let json: Vec<serde_json::Value> = cases
        .iter()
        .map(|(preferred, available, fallback)| serde_json::json!([preferred, available, fallback]))
        .collect();
    let printed = node(&format!(
        "{NEGOTIATE_JS}\nconsole.log(JSON.stringify({}.map(([p, a, f]) => negotiateLocale(p, a, f))));",
        serde_json::Value::from(json)
    ));
    let chosen: Vec<String> = serde_json::from_str(printed.trim()).unwrap();
    for ((preferred, available, fallback), chosen) in cases.iter().zip(chosen) {
        assert_eq!(
            chosen,
            i18n_core::negotiate_locale(preferred, available, fallback),
            "{preferred:?} against {available:?}"
        );
    }
}

#[test]
fn the_root_page_sends_the_reader_to_the_root_of_their_locale() {
    let html = root_page(&site(""), &pages(), "es", &titles());
    let script = html
        .split("<script>")
        .nth(1)
        .and_then(|rest| rest.split("</script>").next())
        .unwrap();
    let run = |languages: &str| {
        node(&format!(
            "const location = {{ search: \"?telar-renderer=dom\", hash: \"#team\", replace(to) {{ console.log(to); }} }};\nnew Function(\"navigator\", \"location\", {script})({{ languages: {languages}, language: \"x\" }}, location);",
            script = serde_json::Value::from(script)
        ))
    };
    assert_eq!(
        run("[\"en-US\", \"es\"]").trim(),
        "/en/?telar-renderer=dom#team"
    );
    assert_eq!(run("[\"fr\"]").trim(), "/es/?telar-renderer=dom#team");
    assert_eq!(run("[]").trim(), "/es/?telar-renderer=dom#team");
}

#[test]
fn without_scripts_the_root_page_offers_every_locale_and_goes_to_the_base() {
    let html = root_page(&site(""), &pages(), "es", &titles());
    for expected in [
        r#"<html lang="es" dir="ltr">"#,
        "<title>Portafolio</title>",
        r#"<noscript><meta http-equiv="refresh" content="0; url=/es/" /></noscript>"#,
        r#"<link rel="alternate" href="/es/" hreflang="es" />"#,
        r#"<link rel="alternate" href="/en/" hreflang="en" />"#,
        r#"<link rel="alternate" href="/es/" hreflang="x-default" />"#,
        r#"<li><a href="/en/" hreflang="en" lang="en">Portfolio</a></li>"#,
        r#"<li><a href="/es/" hreflang="es" lang="es">Portafolio</a></li>"#,
    ] {
        assert!(html.contains(expected), "{expected} in\n{html}");
    }
    assert!(!html.contains("canonical"), "{html}");
}

#[test]
fn a_root_page_of_a_site_with_an_origin_points_at_the_base_locale() {
    let html = root_page(
        &site("origin = \"https://example.com\"\ntheme_color = \"#111\""),
        &pages(),
        "es",
        &titles(),
    );
    for expected in [
        r#"<link rel="canonical" href="https://example.com/es/" />"#,
        r#"<link rel="alternate" href="https://example.com/en/" hreflang="en" />"#,
        r##"<meta name="theme-color" content="#111" />"##,
    ] {
        assert!(html.contains(expected), "{expected} in\n{html}");
    }
    assert_eq!(html.matches("hreflang=\"en\" />").count(), 1, "{html}");
}

#[test]
fn a_root_page_under_a_base_path_sends_the_reader_below_it() {
    let html = root_page(&site("base = \"/docs/\""), &pages(), "es", &titles());
    assert!(html.contains(r#"<base href="/docs/" />"#), "{html}");
    assert!(html.contains("url=/docs/es/"), "{html}");
    assert!(
        html.contains(r#"location.replace("/docs/" + locale"#),
        "{html}"
    );
}
