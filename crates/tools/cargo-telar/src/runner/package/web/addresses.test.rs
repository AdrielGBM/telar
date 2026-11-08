use telar_project::WebSection;

use super::super::page::{Bootstrap, DEFAULT_TEMPLATE};
use super::*;

fn at(segments: &[&str]) -> PageLocation {
    PageLocation {
        segments: segments.iter().map(|segment| segment.to_string()).collect(),
        locale: None,
    }
}

fn site(toml: &str) -> Site {
    let web: WebSection = toml::from_str(toml).expect("a web table parses");
    Site::new(&web, Path::new("/nonexistent"), "demo").unwrap()
}

fn page(site: &Site) -> Page {
    let mut page = Page::new(
        "demo",
        Bootstrap {
            glue: "app-0123456789ab.js".to_string(),
            module: "app_bg-ba9876543210.wasm".to_string(),
        },
    );
    page.base = site.base().to_string();
    page
}

fn output(name: &str) -> PathBuf {
    let out = std::env::temp_dir().join(format!("telar_addresses_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    out
}

fn read(out: &Path, file: &str) -> String {
    std::fs::read_to_string(out.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

#[test]
fn the_page_is_written_at_every_address_once_and_at_404() {
    let out = output("every");
    let site = site("");
    let pages = write_pages(
        &out,
        &page(&site),
        DEFAULT_TEMPLATE,
        &site,
        &[
            at(&["preview", "demo--card--a"]),
            at(&["matrix", "demo--card--a"]),
            at(&["docs", "Forms", "Text field"]),
            at(&["preview", "demo--card--a"]),
            PageLocation::root(),
        ],
    )
    .unwrap();

    assert_eq!(
        pages.pages,
        [
            PageLocation::root(),
            at(&["preview", "demo--card--a"]),
            at(&["matrix", "demo--card--a"]),
            at(&["docs", "Forms", "Text field"]),
        ]
    );
    assert_eq!(pages.page_dirs, ["preview", "matrix", "docs"]);
    assert!(pages.not_found && pages.locales.is_empty());
    for file in [
        "preview/demo--card--a/index.html",
        "matrix/demo--card--a/index.html",
        "docs/Forms/Text field/index.html",
        "404.html",
    ] {
        let html = read(&out, file);
        assert!(
            html.contains("import init from \"/app-0123456789ab.js\";"),
            "{file} loads the app from the site's root: {html}"
        );
    }
    assert!(
        !out.join("index.html").exists(),
        "the root is the build's own"
    );
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn an_address_no_file_can_hold_is_left_to_404() {
    let out = output("unholdable");
    let site = site("");
    let pages = write_pages(
        &out,
        &page(&site),
        DEFAULT_TEMPLATE,
        &site,
        &[at(&["docs", ".."]), at(&["preview", "a"])],
    )
    .unwrap();
    assert_eq!(pages.pages, [PageLocation::root(), at(&["preview", "a"])]);
    assert!(out.join("404.html").is_file());
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn each_page_is_described_at_its_own_address_and_404_at_none() {
    let out = output("described");
    let site = site("origin = \"https://example.com\"\nbase = \"/workshop/\"");
    write_pages(
        &out,
        &page(&site),
        DEFAULT_TEMPLATE,
        &site,
        &[at(&["preview", "demo--card--a"])],
    )
    .unwrap();

    let preview = read(&out, "preview/demo--card--a/index.html");
    assert!(
        preview.contains(r#"<base href="/workshop/" />"#),
        "{preview}"
    );
    assert!(
        preview.contains(
            r#"<link rel="canonical" href="https://example.com/workshop/preview/demo--card--a/" />"#
        ),
        "{preview}"
    );
    assert!(
        preview.contains("import init from \"/workshop/app-0123456789ab.js\";"),
        "{preview}"
    );
    let not_found = read(&out, "404.html");
    assert!(
        not_found.contains(r#"<base href="/workshop/" />"#),
        "{not_found}"
    );
    assert!(!not_found.contains("rel=\"canonical\""), "{not_found}");
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn a_page_already_in_the_output_is_not_overwritten() {
    let out = output("taken");
    std::fs::create_dir_all(out.join("preview/a")).unwrap();
    std::fs::write(out.join("preview/a/index.html"), "public").unwrap();
    let site = site("");
    assert!(
        write_pages(
            &out,
            &page(&site),
            DEFAULT_TEMPLATE,
            &site,
            &[at(&["preview", "a"])]
        )
        .is_err()
    );
    let _ = std::fs::remove_dir_all(&out);
}
