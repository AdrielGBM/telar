use super::*;

fn page() -> Page {
    Page::new(
        "demo",
        Bootstrap {
            glue: "app-0123456789ab.js".to_string(),
            module: "app_bg-ba9876543210.wasm".to_string(),
        },
    )
}

fn minimal(extra: &str) -> String {
    format!("<head>\n  %telar.bootstrap%\n</head>\n{extra}")
}

fn face(path: &str, preload: bool) -> PageFont {
    PageFont {
        declaration: toml::from_str::<telar_project::TelarManifest>(
            "[[telar.fonts]]\nfamily = \"A\"\nsrc = \"a.woff2\"\n",
        )
        .unwrap()
        .telar
        .fonts
        .remove(0),
        path: path.to_string(),
        preload: preload.then(|| "font/woff2".to_string()),
    }
}

#[test]
fn the_built_in_page_expands_with_nothing_fed_but_the_bootstrap() {
    let html = page()
        .render(DEFAULT_TEMPLATE)
        .expect("the built-in page expands");
    assert!(!html.contains("%telar."), "every marker expanded:\n{html}");
    assert!(html.contains(r#"<html lang="en" dir="ltr">"#));
    assert!(html.contains("<title>demo</title>"));
    assert!(html.contains(r#"<link rel="modulepreload" href="/app-0123456789ab.js" />"#));
    assert!(html.contains(
        r#"<link rel="preload" href="/app_bg-ba9876543210.wasm" as="fetch" type="application/wasm" crossorigin />"#
    ));
    assert!(html.contains(r#"import init from "/app-0123456789ab.js";"#));
    assert!(html.contains("wasm.telar_start();"));
    assert!(html.contains(r#"<div id="telar-root" data-telar-renderer="auto"></div>"#));
    assert!(!html.contains("telar-state"), "no state, no script");
}

/// Under the document renderer the page's primary scroll is the document's own, so the page must leave the document free to scroll, including before the app has loaded.
#[test]
fn the_built_in_page_lets_the_document_scroll() {
    assert!(DEFAULT_TEMPLATE.contains("html, body { margin: 0; height: 100%; }"));
    assert!(!DEFAULT_TEMPLATE.contains("overflow"));
}

#[test]
fn a_block_marker_that_writes_nothing_takes_its_line_with_it() {
    let html = page().render(DEFAULT_TEMPLATE).unwrap();
    for gone in ["%telar.fonts%", "%telar.state%"] {
        assert!(!html.contains(gone));
    }
    assert!(
        !html.lines().any(|line| line.trim().is_empty()),
        "no blank line is left where a marker wrote nothing:\n{html}"
    );
}

#[test]
fn a_block_marker_indents_every_line_it_writes_like_its_own() {
    let html = page().render(&minimal("")).unwrap();
    let script: Vec<&str> = html
        .lines()
        .skip_while(|line| !line.contains("<script"))
        .take(5)
        .collect();
    assert_eq!(
        script,
        [
            "  <script type=\"module\">",
            "    import init from \"/app-0123456789ab.js\";",
            "    const wasm = await init();",
            "    wasm.telar_start();",
            "  </script>",
        ]
    );
}

#[test]
fn every_fed_marker_lands_where_the_template_puts_it() {
    let mut page = page();
    page.lang = "es".to_string();
    page.dir = Direction::Rtl;
    page.title = "Portafolio".to_string();
    page.renderer = Some(WebRenderer::Dom);
    page.meta.push(HeadTag::meta("description", "Hola"));
    page.meta
        .push(HeadTag::link("canonical", "https://example.com/es/"));
    page.fonts.push(face("fonts/a-0123.woff2", true));
    page.prerendered = "<main data-telar-id=\"1\">Hola</main>".to_string();
    page.state = Some(serde_json::json!({ "locale": "es" }));

    let html = page.render(DEFAULT_TEMPLATE).unwrap();
    assert!(html.contains(r#"<html lang="es" dir="rtl">"#));
    assert!(html.contains("<title>Portafolio</title>"));
    assert!(html.contains(r#"<meta name="description" content="Hola" />"#));
    assert!(html.contains(r#"<link rel="canonical" href="https://example.com/es/" />"#));
    assert!(html.contains(
        r#"<link rel="preload" href="/fonts/a-0123.woff2" as="font" type="font/woff2" crossorigin data-telar-family="A" />"#
    ));
    assert!(html.contains(r#"src: url("/fonts/a-0123.woff2") format("woff2");"#));
    assert!(html.contains(
        r#"<div id="telar-root" data-telar-renderer="dom"><main data-telar-id="1">Hola</main></div>"#
    ));
    assert!(
        html.contains(
            r#"<script type="application/json" id="telar-state">{"locale":"es"}</script>"#
        )
    );
}

#[test]
fn a_different_base_moves_every_url_the_page_writes() {
    let mut page = page();
    page.base = "/portfolio/".to_string();
    let html = page.render(&minimal("")).unwrap();
    assert!(html.contains(r#"href="/portfolio/app-0123456789ab.js""#));
    assert!(html.contains(r#"href="/portfolio/app_bg-ba9876543210.wasm""#));
    assert!(html.contains(r#"import init from "/portfolio/app-0123456789ab.js";"#));
}

#[test]
fn text_and_attribute_values_are_escaped() {
    let mut page = page();
    page.title = "Tom & \"Jerry\" <3".to_string();
    page.meta
        .push(HeadTag::meta("description", "a \"quoted\" <tag>"));
    let html = page
        .render(&minimal("<title>%telar.title%</title>\n%telar.meta%"))
        .unwrap();
    assert!(html.contains("<title>Tom &amp; &quot;Jerry&quot; &lt;3</title>"));
    assert!(html.contains(r#"content="a &quot;quoted&quot; &lt;tag&gt;""#));
}

/// State is written into a `<script>`, where a `</script>` inside a string would end the element early.
#[test]
fn state_cannot_close_its_own_script() {
    let mut page = page();
    page.state = Some(serde_json::json!({ "note": "</script><script>alert(1)</script>" }));
    let html = page.render(&minimal("%telar.state%")).unwrap();
    assert_eq!(html.matches("</script>").count(), 2, "{html}");
    let lt = format!("\\u{:04x}", '<' as u32);
    assert!(html.contains(&format!(
        r#"{{"note":"{lt}/script>{lt}script>alert(1){lt}/script>"}}"#
    )));
}

#[test]
fn a_value_marker_can_appear_any_number_of_times() {
    let html = page()
        .render(&minimal("<p>%telar.title%</p><p>%telar.title%</p>"))
        .unwrap();
    assert!(html.contains("<p>demo</p><p>demo</p>"));
}

#[test]
fn text_that_only_resembles_a_marker_is_left_alone() {
    let text = "width: 100%; 50%telar.x %telar.Title% %telar.%";
    let html = page().render(&minimal(text)).unwrap();
    assert!(html.contains(text));
}

#[test]
fn an_unknown_marker_is_an_error_naming_it_and_its_line() {
    let error = page()
        .render(&minimal("<p>%telar.titel%</p>"))
        .expect_err("a misspelled marker must not pass as text");
    assert_eq!(
        error,
        TemplateError::Unknown {
            name: "titel".to_string(),
            line: 4,
        }
    );
    assert!(error.to_string().contains("%telar.title%"), "{error}");
}

#[test]
fn a_template_that_never_loads_the_app_is_refused() {
    let error = page().render("<html></html>").unwrap_err();
    assert_eq!(error, TemplateError::Missing(Marker::Bootstrap));
}

#[test]
fn a_block_marker_twice_is_refused() {
    let error = page().render(&minimal("%telar.bootstrap%")).unwrap_err();
    assert_eq!(error, TemplateError::Repeated(Marker::Bootstrap));
}

/// A template may leave out what the build has nothing for, but not what it has: dropping prerendered markup, state or a font would break the page silently.
#[test]
fn a_marker_left_out_is_an_error_only_when_the_build_has_content_for_it() {
    let template = minimal("");
    assert!(page().render(&template).is_ok());

    let mut prerendered = page();
    prerendered.prerendered = "<main></main>".to_string();
    assert_eq!(
        prerendered.render(&template),
        Err(TemplateError::Missing(Marker::Prerendered))
    );

    let mut state = page();
    state.state = Some(serde_json::json!({}));
    assert_eq!(
        state.render(&template),
        Err(TemplateError::Missing(Marker::State))
    );

    let mut fonts = page();
    fonts.fonts.push(face("fonts/a-0123.woff2", false));
    assert_eq!(
        fonts.render(&template),
        Err(TemplateError::Missing(Marker::Fonts))
    );
}

#[test]
fn a_prerendered_page_marks_its_host_and_styles_its_content() {
    let mut page = page();
    page.host = vec![
        ("data-telar".to_string(), String::new()),
        ("style".to_string(), "position:relative;".to_string()),
    ];
    page.head
        .push("<style id=\"telar-reset\">[data-telar]{}</style>".to_string());
    page.prerendered = "<main data-telar-id=\"1\"></main>".to_string();
    let html = page.render(DEFAULT_TEMPLATE).unwrap();
    assert!(html.contains(
        r#"<div id="telar-root" data-telar-renderer="auto" data-telar="" style="position:relative;"><main data-telar-id="1"></main></div>"#
    ), "{html}");
    assert!(
        html.contains("    <style id=\"telar-reset\">[data-telar]{}</style>\n"),
        "{html}"
    );
}

#[test]
fn a_template_with_no_host_marker_cannot_carry_a_prerendered_host() {
    let mut page = page();
    page.host = vec![("data-telar".to_string(), String::new())];
    assert_eq!(
        page.render(&minimal("")),
        Err(TemplateError::Missing(Marker::Host))
    );
    let twice = minimal("<div %telar.host%></div><div %telar.host%></div>");
    assert_eq!(
        page.render(&twice),
        Err(TemplateError::Repeated(Marker::Host))
    );
}
