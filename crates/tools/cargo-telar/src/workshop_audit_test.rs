//! The exported workshop audited the way an accessibility review would: axe-core's WCAG 2.2 A/AA and best-practice rules, the ones `plugins/telar-components/src/audit_test.rs` holds the widgets to (`crates/renderer/renderer-dom/axe.json`), run in Chrome over the canvas of every preview and matrix page of a `cargo telar preview --build <dir> --target web` export, and over the workshop's own chrome on one of them.
//!
//! It needs the export, Chrome and its driver, and axe-core's script, so it is ignored by default. Inside the dev shell, which sets `CHROMEDRIVER` and `TELAR_AXE_CORE`, run it with `TELAR_WORKSHOP_SITE=<dir> cargo test -p cargo-telar --test workshop_audit -- --ignored`; a relative `<dir>` is read from the repository root.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};
use telar_project::protocol::preview_location;

#[path = "served_site_test.rs"]
mod served_site;

use served_site::{repo_root, served_at};

const SITE_VAR: &str = "TELAR_WORKSHOP_SITE";

/// Narrows the audit to the pages whose address, query included, contains its value.
const ONLY_VAR: &str = "TELAR_WORKSHOP_AUDIT_ONLY";

const AXE_OPTIONS: &str = include_str!("../../../renderer/renderer-dom/axe.json");

/// Asks each page to draw as a document whatever the browser offers: pixels on a canvas give axe nothing to read.
const AS_DOCUMENT: &str = "telar-renderer=dom";

/// What `telar-platform-web` keeps on the app's host while its loop has no frame coming (its `IDLE_ATTRIBUTE`).
const IDLE_ATTRIBUTE: &str = "data-telar-idle";

/// How long the host has to stay idle, with every font loaded, before the page counts as drawn: a caret's blink or a face landing late wakes the loop for a frame, and that frame must not be taken for the last one.
const QUIET: Duration = Duration::from_millis(300);

/// How long a page has to load its module and settle.
const SETTLE_LIMIT: Duration = Duration::from_secs(30);

/// Answers with the host once it has stayed idle for the quiet period, or with why it did not within the limit.
const SETTLE: &str = r#"
const [attribute, quiet, limit, done] = arguments;
const started = performance.now();
let idled = false;
let since = null;
const check = () => {
  const now = performance.now();
  const host = document.querySelector(`[${attribute}]`);
  idled = idled || host !== null;
  if (host === null || document.fonts.status !== "loaded") {
    since = null;
  } else if (since === null) {
    since = now;
  } else if (now - since >= quiet) {
    return done(host);
  }
  if (now - started >= limit) {
    return done(idled
      ? `the app never stayed idle for ${quiet} ms in ${limit} ms: something wakes it over and over`
      : `the app never went idle in ${limit} ms: it did not start, or it animates without end though the page prefers reduced motion`);
  }
  setTimeout(check, 25);
};
check();
"#;

/// Puts axe-core into the page, as `audit_test.rs` does: a script element carrying its source.
const INJECT: &str = r#"
if (window.axe === undefined) {
  const script = document.createElement("script");
  script.textContent = arguments[0];
  document.head.appendChild(script);
}
return typeof window.axe;
"#;

/// Runs axe over the element given, or the whole document for `null`, and answers with each violation, or with why it could not run.
const RUN: &str = r#"
const [context, options, done] = arguments;
axe.run(context ?? document, options).then(
  (results) => done(results.violations.map((violation) => ({
    id: violation.id,
    impact: violation.impact ?? "",
    help: violation.help,
    helpUrl: violation.helpUrl,
    nodes: violation.nodes.map((node) => ({
      target: node.target.map(String).join(" "),
      html: node.html,
      summary: node.failureSummary ?? "",
    })),
  }))),
  (error) => done(String(error)),
);
"#;

#[test]
#[ignore = "audits an exported workshop in Chrome: run it with `TELAR_WORKSHOP_SITE=<dir> cargo test -p cargo-telar --test workshop_audit -- --ignored`"]
fn every_preview_page_of_the_exported_workshop_passes_axe() {
    let site = site_dir();
    let axe = axe_script();
    let previews = preview_pages(&site);
    let Some(first) = previews.first() else {
        panic!(
            "{} holds no preview pages; is it a workshop export?",
            site.display()
        );
    };
    let mut audits: Vec<Audit> = previews
        .iter()
        .map(|address| Audit {
            address: format!("{address}?chrome=0&{AS_DOCUMENT}"),
            scope: Scope::Canvas,
        })
        .collect();
    audits.push(Audit {
        address: format!("{first}?{AS_DOCUMENT}"),
        scope: Scope::Page,
    });
    if let Some(only) = std::env::var_os(ONLY_VAR) {
        let only = only.to_string_lossy();
        audits.retain(|audit| audit.address.contains(only.as_ref()));
        assert!(!audits.is_empty(), "no page's address contains `{only}`");
    }

    let origin = serve(site.clone());
    let driver = Driver::start();
    let session = driver.session();
    let mut reports = Vec::new();
    for (done, audit) in audits.iter().enumerate() {
        eprintln!("[{}/{}] {}", done + 1, audits.len(), audit.address);
        if let Err(report) = session.audit(&origin, audit, &axe) {
            reports.push(format!("{}\n{report}", audit.address));
        }
    }
    assert!(
        reports.is_empty(),
        "axe found problems on {} of the {} pages audited in {}:\n\n{}",
        reports.len(),
        audits.len(),
        site.display(),
        reports.join("\n\n")
    );
}

struct Audit {
    address: String,
    scope: Scope,
}

enum Scope {
    /// The canvas alone, one fragment of a page, audited within the app's host as `audit_test.rs` audits its fixture.
    Canvas,
    /// The workshop's chrome around the canvas, audited as the whole page it is, which has to sit in landmarks.
    Page,
}

impl Scope {
    fn options(&self) -> Value {
        let mut options: Value =
            serde_json::from_str(AXE_OPTIONS).expect("renderer-dom/axe.json is JSON");
        if let Scope::Page = self {
            options["rules"]["region"]["enabled"] = Value::Bool(true);
        }
        options
    }
}

#[derive(Deserialize)]
struct Violation {
    id: String,
    impact: String,
    help: String,
    #[serde(rename = "helpUrl")]
    help_url: String,
    nodes: Vec<ViolatingNode>,
}

#[derive(Deserialize)]
struct ViolatingNode {
    target: String,
    html: String,
    summary: String,
}

/// Each violation, one paragraph each, naming the rule, every element that breaks it and what is wrong with each.
fn describe(violations: &[Violation]) -> String {
    let mut text = String::new();
    for violation in violations {
        text.push_str(&format!(
            "  {} ({}): {}\n    {}\n",
            violation.id, violation.impact, violation.help, violation.help_url
        ));
        for node in &violation.nodes {
            text.push_str(&format!(
                "    {}: {}\n      {}\n",
                node.target,
                shortened(&node.html),
                node.summary.replace('\n', "\n      ")
            ));
        }
    }
    text
}

fn shortened(html: &str) -> String {
    const LIMIT: usize = 200;
    match html.char_indices().nth(LIMIT) {
        Some((end, _)) => format!("{}…", &html[..end]),
        None => html.to_string(),
    }
}

fn site_dir() -> PathBuf {
    let dir = std::env::var_os(SITE_VAR).unwrap_or_else(|| {
        panic!(
            "{SITE_VAR} names no exported workshop: export one with `cargo telar preview -p catalogue --build <dir> --target web` and set it to that directory"
        )
    });
    let dir = repo_root().join(dir);
    assert!(
        dir.join("index.html").is_file(),
        "{} ({SITE_VAR}) holds no exported site",
        dir.display()
    );
    dir
}

fn axe_script() -> String {
    let path = std::env::var_os("TELAR_AXE_CORE").unwrap_or_else(|| {
        panic!(
            "TELAR_AXE_CORE names axe-core's script; the repository's dev shell sets it, so run inside `nix develop`"
        )
    });
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read axe-core from {}: {e}",
            Path::new(&path).display()
        )
    })
}

/// The address of every preview page of the site and then of every matrix page, each in the order of its id.
fn preview_pages(site: &Path) -> Vec<String> {
    let mut pages = Vec::new();
    for kind in ["preview", "matrix"] {
        let Ok(entries) = std::fs::read_dir(site.join(kind)) else {
            continue;
        };
        let mut ids: Vec<String> = entries
            .flatten()
            .filter(|entry| entry.path().join("index.html").is_file())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        ids.sort();
        pages.extend(ids.iter().map(|id| {
            let preview = preview_location(id);
            match kind {
                "preview" => preview,
                _ => format!("/matrix/{}", preview.trim_start_matches("/preview/")),
            }
        }));
    }
    pages
}

/// Serves `site` as a static host would, on a loopback port, and answers with its origin. An address no file answers gets the site's `404.html`.
fn serve(site: PathBuf) -> String {
    let listener =
        TcpListener::bind(("127.0.0.1", 0)).expect("a loopback port to serve the site on");
    let origin = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().expect("a bound address").port()
    );
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let site = site.clone();
            std::thread::spawn(move || answer(stream, &site));
        }
    });
    origin
}

fn answer(mut stream: TcpStream, site: &Path) {
    let Ok(reading) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(reading);
    let mut request = String::new();
    if reader.read_line(&mut request).is_err() {
        return;
    }
    let target = request.split_whitespace().nth(1).unwrap_or("/").to_string();
    let mut header = String::new();
    while reader.read_line(&mut header).is_ok_and(|read| read > 0) && !header.trim().is_empty() {
        header.clear();
    }
    let (status, file) = match served_at(site, &target).filter(|file| file.is_file()) {
        Some(file) => ("200 OK", file),
        None => ("404 Not Found", site.join("404.html")),
    };
    let body = std::fs::read(&file).unwrap_or_default();
    let head = format!(
        "HTTP/1.1 {status}\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        media_type(&file),
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
}

fn media_type(file: &Path) -> &'static str {
    match file.extension().and_then(|extension| extension.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// A chromedriver of its own, on a loopback port, stopped when dropped.
struct Driver {
    process: Child,
    client: Client,
}

impl Driver {
    fn start() -> Self {
        let program = std::env::var_os("CHROMEDRIVER").unwrap_or_else(|| "chromedriver".into());
        let port = TcpListener::bind(("127.0.0.1", 0))
            .and_then(|listener| listener.local_addr())
            .expect("a free loopback port for the driver")
            .port();
        let process = Command::new(&program)
            .arg(format!("--port={port}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|e| {
                panic!(
                    "could not start {} (set CHROMEDRIVER, as the dev shell does): {e}",
                    Path::new(&program).display()
                )
            });
        let driver = Self {
            process,
            client: Client::new(format!("http://127.0.0.1:{port}")),
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        while driver
            .client
            .send("GET", "/status", None)
            .ok()
            .and_then(|status| status["ready"].as_bool())
            != Some(true)
        {
            assert!(
                Instant::now() < deadline,
                "chromedriver did not come up on port {port}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        driver
    }

    /// A headless Chrome that prefers reduced motion, under which a looping animation stops rather than keeping the app from ever going idle: an app that loops on regardless fails the audit.
    fn session(&self) -> Session<'_> {
        let capabilities = json!({
            "capabilities": {
                "alwaysMatch": {
                    "browserName": "chrome",
                    "goog:chromeOptions": {
                        "args": ["--headless=new", "--no-sandbox", "--disable-gpu", "--disable-dev-shm-usage", "--window-size=1280,900", "--force-prefers-reduced-motion"]
                    },
                    "goog:loggingPrefs": { "browser": "ALL" }
                }
            }
        });
        let created = self
            .client
            .send("POST", "/session", Some(capabilities))
            .unwrap_or_else(|e| panic!("chromedriver opened no browser: {e}"));
        let id = created["sessionId"]
            .as_str()
            .expect("a new session names itself")
            .to_string();
        let session = Session {
            client: &self.client,
            path: format!("/session/{id}"),
        };
        let wait = SETTLE_LIMIT + Duration::from_secs(60);
        session
            .send(
                "POST",
                "/timeouts",
                json!({ "script": wait.as_millis() as u64, "pageLoad": wait.as_millis() as u64 }),
            )
            .unwrap_or_else(|e| panic!("the session took no timeouts: {e}"));
        session
    }
}

impl Drop for Driver {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// One browser, driven over WebDriver, closed when dropped.
struct Session<'a> {
    client: &'a Client,
    path: String,
}

impl Session<'_> {
    fn send(&self, method: &str, path: &str, body: Value) -> Result<Value, String> {
        self.client
            .send(method, &format!("{}{path}", self.path), Some(body))
    }

    /// Loads the page at `audit`'s address, waits for the app to settle, and runs axe over what the audit covers. `Err` describes each violation, or why the page could not be audited.
    fn audit(&self, origin: &str, audit: &Audit, axe: &str) -> Result<(), String> {
        self.send(
            "POST",
            "/url",
            json!({ "url": format!("{origin}{}", audit.address) }),
        )?;
        let host = self.send(
            "POST",
            "/execute/async",
            json!({
                "script": SETTLE,
                "args": [IDLE_ATTRIBUTE, QUIET.as_millis() as u64, SETTLE_LIMIT.as_millis() as u64],
            }),
        )?;
        if let Some(reason) = host.as_str() {
            return Err(format!("  {reason}\n{}", self.console()));
        }
        let loaded = self.send(
            "POST",
            "/execute/sync",
            json!({ "script": INJECT, "args": [axe] }),
        )?;
        if loaded != "object" {
            return Err(format!("  axe-core did not load into the page ({loaded})"));
        }
        let context = match audit.scope {
            Scope::Canvas => host,
            Scope::Page => Value::Null,
        };
        let found = self.send(
            "POST",
            "/execute/async",
            json!({ "script": RUN, "args": [context, audit.scope.options()] }),
        )?;
        if let Some(reason) = found.as_str() {
            return Err(format!("  axe could not run: {reason}"));
        }
        let violations: Vec<Violation> = serde_json::from_value(found)
            .map_err(|e| format!("  axe answered with something unexpected: {e}"))?;
        match violations.is_empty() {
            true => Ok(()),
            false => Err(describe(&violations)),
        }
    }

    /// What the page wrote to its console, which is where a panic in the app ends up.
    fn console(&self) -> String {
        let entries = self
            .send("POST", "/se/log", json!({ "type": "browser" }))
            .unwrap_or_default();
        let lines: Vec<String> = entries
            .as_array()
            .into_iter()
            .flatten()
            .map(|entry| {
                format!(
                    "    {} {}",
                    entry["level"].as_str().unwrap_or("?"),
                    entry["message"].as_str().unwrap_or("")
                )
            })
            .collect();
        match lines.is_empty() {
            true => "  the console is empty".to_string(),
            false => format!("  console:\n{}", lines.join("\n")),
        }
    }
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        let _ = self.client.send("DELETE", &self.path, None);
    }
}

/// A WebDriver endpoint, spoken to in JSON.
struct Client {
    agent: ureq::Agent,
    origin: String,
}

impl Client {
    fn new(origin: String) -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(SETTLE_LIMIT + Duration::from_secs(120)))
            .build()
            .into();
        Self { agent, origin }
    }

    /// The `value` of the answer to one command, or the error the driver answered with.
    fn send(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
        let url = format!("{}{path}", self.origin);
        let sent = match (method, body) {
            ("GET", _) => self.agent.get(&url).call(),
            ("DELETE", _) => self.agent.delete(&url).call(),
            (_, body) => self
                .agent
                .post(&url)
                .content_type("application/json")
                .send(body.unwrap_or_else(|| json!({})).to_string()),
        };
        let mut response = sent.map_err(|e| format!("{method} {path}: {e}"))?;
        let text = response
            .body_mut()
            .with_config()
            .limit(64 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| format!("{method} {path}: {e}"))?;
        let mut answer: Value =
            serde_json::from_str(&text).map_err(|e| format!("{method} {path}: {e}: {text}"))?;
        let value = answer["value"].take();
        match value["error"].as_str() {
            Some(error) => Err(format!(
                "{method} {path}: {error}: {}",
                value["message"].as_str().unwrap_or("")
            )),
            None => Ok(value),
        }
    }
}
