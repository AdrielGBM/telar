use std::path::PathBuf;
use std::process::Command;

use super::*;

fn site() -> Site {
    let web: telar_project::WebSection =
        toml::from_str("origin = \"https://example.com\"\nhost = \"cloudflare-pages\"").unwrap();
    Site::new(&web, Path::new("/nonexistent"), "demo").unwrap()
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

/// An output directory holding the hashed bootstrap, its manifest and `files`.
fn output(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let out = std::env::temp_dir().join(format!("telar_cloudflare_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        out.join(MANIFEST_FILE),
        r#"{"app.js":"app-0123456789ab.js","app_bg.wasm":"app_bg-ba9876543210.wasm"}"#,
    )
    .unwrap();
    for (path, content) in files {
        let file = out.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content).unwrap();
    }
    out
}

fn read(out: &Path, name: &str) -> String {
    std::fs::read_to_string(out.join(name)).unwrap()
}

#[test]
fn hashed_files_are_cached_for_good_and_pages_revalidate() {
    let hashed = ["app-0123456789ab.js".to_string()];
    let rules = headers(&site(), &localized(), &hashed, true);
    assert_eq!(
        rules,
        "/\n  Cache-Control: no-cache\n  Vary: Accept-Language\n\
         /404.html\n  Cache-Control: no-cache\n\
         /es/*\n  Cache-Control: no-cache\n\
         /en/*\n  Cache-Control: no-cache\n\
         /app-0123456789ab.js\n  Cache-Control: public, max-age=31536000, immutable\n\
         /images/*\n  Cache-Control: public, max-age=31536000, immutable\n"
    );
}

#[test]
fn a_site_without_locales_names_each_page_and_varies_by_nothing() {
    let pages = SitePages {
        pages: vec![PageLocation::root(), at(&["projects"], None)],
        not_found: false,
        ..SitePages::default()
    };
    let rules = headers(&site(), &pages, &[], false);
    assert_eq!(
        rules,
        "/\n  Cache-Control: no-cache\n/projects/\n  Cache-Control: no-cache\n"
    );
    assert_eq!(redirects(&site(), &pages), "");
}

#[test]
fn an_address_naming_no_locale_goes_to_the_base_locale() {
    assert_eq!(
        redirects(&site(), &localized()),
        "/projects /es/projects/ 302\n/projects/ /es/projects/ 302\n"
    );
}

#[test]
fn the_project_files_come_first_and_the_derived_rules_after() {
    let out = output(
        "merge",
        &[
            ("_headers", "/*\n  X-Frame-Options: DENY"),
            ("_redirects", "/old /es/ 301\n"),
        ],
    );
    write_host_files(&out, &site(), &localized()).unwrap();
    let headers = read(&out, HEADERS_FILE);
    assert!(
        headers.starts_with("/*\n  X-Frame-Options: DENY\n/\n  Cache-Control: no-cache\n"),
        "{headers}"
    );
    assert!(
        headers.contains(
            "/app_bg-ba9876543210.wasm\n  Cache-Control: public, max-age=31536000, immutable\n"
        ),
        "{headers}"
    );
    let redirects = read(&out, REDIRECTS_FILE);
    assert!(
        redirects.starts_with("/old /es/ 301\n/projects /es/projects/ 302\n"),
        "{redirects}"
    );
    assert_eq!(
        read(&out, ROUTES_FILE),
        "{\"version\":1,\"include\":[\"/\"],\"exclude\":[]}\n"
    );
    assert!(read(&out, WORKER_FILE).contains("export default"));
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn a_site_without_locales_needs_no_worker() {
    let out = output("plain", &[]);
    write_host_files(&out, &site(), &SitePages::root_only()).unwrap();
    assert!(out.join(HEADERS_FILE).is_file());
    assert!(!out.join(REDIRECTS_FILE).exists());
    assert!(!out.join(WORKER_FILE).exists());
    assert!(!out.join(ROUTES_FILE).exists());
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn more_rules_than_pages_reads_is_an_error() {
    let own: String = (0..100)
        .map(|i| format!("/own-{i}\n  X-Own: {i}\n"))
        .collect();
    let out = output("too_many", &[("_headers", &own)]);
    let error = write_host_files(&out, &site(), &SitePages::root_only()).unwrap_err();
    let _ = std::fs::remove_dir_all(&out);
    assert!(error.contains("at most 100"), "{error}");
}

#[test]
fn a_file_over_what_pages_serves_is_named() {
    let out = output("oversized", &[]);
    let file = std::fs::File::create(out.join("huge.bin")).unwrap();
    file.set_len(MAX_FILE_BYTES + 1).unwrap();
    let found = oversized(&out).unwrap();
    let _ = std::fs::remove_dir_all(&out);
    assert_eq!(found, [("huge.bin".to_string(), MAX_FILE_BYTES + 1)]);
}

/// The worker, run by Node.js against requests for `/` and for a file, printing each response's status and headers.
#[test]
fn the_worker_redirects_the_root_by_accept_language_and_serves_everything_else() {
    let dir = output("worker", &[]);
    std::fs::write(
        dir.join("worker.mjs"),
        worker(&site(), &["es".to_string(), "en".to_string()], "es"),
    )
    .unwrap();
    std::fs::write(
        dir.join("harness.mjs"),
        r#"import worker from "./worker.mjs";
const env = { ASSETS: { fetch: (request) => new Response("asset " + new URL(request.url).pathname) } };
const ask = async (path, language) => {
  const headers = language === undefined ? {} : { "Accept-Language": language };
  const response = await worker.fetch(new Request("https://example.com" + path, { headers }), env);
  const location = response.headers.get("Location");
  return location === null ? await response.text() : `${response.status} ${location} ${response.headers.get("Vary")}`;
};
for (const [path, language] of [
  ["/", "en-US,en;q=0.9,es;q=0.8"],
  ["/", "fr-CA, es;q=0.5, en;q=0.7"],
  ["/", "de, *;q=0.5"],
  ["/", "en;q=0, es-MX"],
  ["/", undefined],
  ["/?telar-renderer=dom", "EN"],
  ["/app.js", "en"],
]) {
  console.log(await ask(path, language));
}
"#,
    )
    .unwrap();
    let output = Command::new("node")
        .arg(dir.join("harness.mjs"))
        .output()
        .expect("node runs: the dev shell provides it");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "302 /en/ Accept-Language\n\
         302 /en/ Accept-Language\n\
         302 /es/ Accept-Language\n\
         302 /es/ Accept-Language\n\
         302 /es/ Accept-Language\n\
         302 /en/?telar-renderer=dom Accept-Language\n\
         asset /app.js\n"
    );
}
