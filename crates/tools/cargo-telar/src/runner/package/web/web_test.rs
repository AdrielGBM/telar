use std::collections::BTreeMap;

use super::*;

const GLUE: &str = "const url = new URL('app_bg.wasm', import.meta.url);\n";

/// A package root and a staging directory holding what `wasm-bindgen` leaves behind.
fn project(name: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("telar_web_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let package = root.join("package");
    let out = root.join("out");
    std::fs::create_dir_all(&package).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("app.js"), GLUE).unwrap();
    std::fs::write(out.join("app_bg.wasm"), b"\0asm module").unwrap();
    (package, out)
}

fn manifest(out: &Path) -> BTreeMap<String, String> {
    serde_json::from_slice(&std::fs::read(out.join(MANIFEST_FILE)).unwrap()).unwrap()
}

fn cleanup(package: &Path) {
    let _ = std::fs::remove_dir_all(package.parent().unwrap());
}

#[test]
fn a_project_with_nothing_under_web_gets_the_built_in_page_and_hashed_bootstrap() {
    let (package, out) = project("defaults");
    assemble(&out, &package, &WebSection::default(), "demo", None).unwrap();

    let manifest = manifest(&out);
    let glue = &manifest["app.js"];
    let module = &manifest["app_bg.wasm"];
    assert_eq!(manifest.len(), 2);
    assert!(glue.starts_with("app-") && glue.ends_with(".js"), "{glue}");
    assert!(
        module.starts_with("app_bg-") && module.ends_with(".wasm"),
        "{module}"
    );
    assert!(!out.join("app.js").exists() && !out.join("app_bg.wasm").exists());

    let glue_source = std::fs::read_to_string(out.join(glue)).unwrap();
    assert!(
        glue_source.contains(&format!("'{module}'")),
        "the glue fetches the hashed module: {glue_source}"
    );

    let page = std::fs::read_to_string(out.join(PAGE_FILE)).unwrap();
    assert!(page.contains(&format!("import init from \"./{glue}\";")));
    assert!(page.contains(r#"<meta name="description" content="demo, a Telar application." />"#));
    cleanup(&package);
}

#[test]
fn a_new_module_gives_the_glue_a_new_name_too() {
    let (package, out) = project("chain_a");
    assemble(&out, &package, &WebSection::default(), "demo", None).unwrap();
    let first = manifest(&out);
    cleanup(&package);

    let (package, out) = project("chain_b");
    std::fs::write(out.join("app_bg.wasm"), b"\0asm another module").unwrap();
    assemble(&out, &package, &WebSection::default(), "demo", None).unwrap();
    let second = manifest(&out);
    cleanup(&package);

    assert_ne!(first["app_bg.wasm"], second["app_bg.wasm"]);
    assert_ne!(first["app.js"], second["app.js"]);
}

#[test]
fn the_project_template_and_public_directory_are_used() {
    let (package, out) = project("custom");
    std::fs::create_dir_all(package.join("web/public/img")).unwrap();
    std::fs::write(
        package.join("web/index.html"),
        "<html lang=\"%telar.lang%\"><head>%telar.bootstrap%</head><body><main id=\"telar-root\"></main></body></html>\n",
    )
    .unwrap();
    std::fs::write(package.join("web/public/robots.txt"), "User-agent: *\n").unwrap();
    std::fs::write(package.join("web/public/img/og.png"), b"png").unwrap();

    assemble(
        &out,
        &package,
        &WebSection::default(),
        "demo",
        Some(WebRenderer::Dom),
    )
    .unwrap();
    let page = std::fs::read_to_string(out.join(PAGE_FILE)).unwrap();
    assert!(
        page.starts_with("<html lang=\"en\"><head><link rel=\"modulepreload\""),
        "{page}"
    );
    assert!(page.contains("<main id=\"telar-root\"></main>"));
    assert!(out.join("robots.txt").is_file());
    assert!(out.join("img/og.png").is_file());
    assert_eq!(
        manifest(&out).len(),
        2,
        "public files keep their names and stay out of the manifest"
    );
    cleanup(&package);
}

#[test]
fn the_web_table_can_move_the_template_and_the_public_directory() {
    let (package, out) = project("moved");
    std::fs::create_dir_all(package.join("site/static")).unwrap();
    std::fs::write(package.join("site/page.html"), "%telar.bootstrap%\n").unwrap();
    std::fs::write(package.join("site/static/favicon.ico"), b"ico").unwrap();
    let web = WebSection {
        template: Some("site/page.html".to_string()),
        public: Some("site/static".to_string()),
    };
    assemble(&out, &package, &web, "demo", None).unwrap();
    assert!(
        std::fs::read_to_string(out.join(PAGE_FILE))
            .unwrap()
            .starts_with("<link")
    );
    assert!(out.join("favicon.ico").is_file());
    cleanup(&package);
}

#[test]
fn a_named_template_or_public_directory_that_is_not_there_is_an_error() {
    let (package, out) = project("named_missing_template");
    let web = WebSection {
        template: Some("site/page.html".to_string()),
        public: None,
    };
    let error = assemble(&out, &package, &web, "demo", None).unwrap_err();
    assert!(error.contains("site/page.html"), "{error}");
    cleanup(&package);

    let (package, out) = project("named_missing_public");
    let web = WebSection {
        template: None,
        public: Some("static".to_string()),
    };
    let error = assemble(&out, &package, &web, "demo", None).unwrap_err();
    assert!(error.contains("[telar.web] public"), "{error}");
    cleanup(&package);
}

#[test]
fn a_template_error_names_the_template() {
    let (package, out) = project("broken_template");
    std::fs::create_dir_all(package.join("web")).unwrap();
    std::fs::write(
        package.join("web/index.html"),
        "<script src=\"./app.js\"></script>\n",
    )
    .unwrap();
    let error = assemble(&out, &package, &WebSection::default(), "demo", None).unwrap_err();
    assert!(error.contains("web/index.html"), "{error}");
    assert!(error.contains("%telar.bootstrap%"), "{error}");
    cleanup(&package);
}

#[test]
fn glue_that_does_not_name_the_module_is_an_error() {
    let (package, out) = project("foreign_glue");
    std::fs::write(out.join("app.js"), "export default function init() {}\n").unwrap();
    let error = assemble(&out, &package, &WebSection::default(), "demo", None).unwrap_err();
    assert!(error.contains("'app_bg.wasm'"), "{error}");
    cleanup(&package);
}

#[test]
fn publishing_replaces_the_previous_output_whole() {
    let (package, staging) = project("publish");
    let out = staging.with_file_name("web");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("app-000000000000.js"), "stale").unwrap();
    publish(&staging, &out).unwrap();
    assert!(!staging.exists());
    assert!(!out.join("app-000000000000.js").exists());
    assert!(out.join("app.js").is_file());
    cleanup(&package);
}
