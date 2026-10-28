//! `cargo telar dev --target web` and `cargo telar preview --target web`: build, serve, rebuild on change, and tell the page to reload.

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use notify::{EventKind, RecursiveMode};

use super::cli::WebRenderer;
use super::config::{TelarSection, find_package_dir, resolve_package};
use super::package::{build_web_bundle, compressible, media_type};
use super::watch::{ChangeFeed, WatchScope, WatchSet, idle};
use super::workshop_channel::ServerClaim;

/// How long to let a burst of file events settle before rebuilding. An editor writing a file produces several, and a save that touches a whole directory produces one per file.
const SETTLE: Duration = Duration::from_millis(150);

/// Bumped by every successful rebuild. The page polls it and reloads when it moves, which is the whole live-reload protocol: no websocket, no client library, and nothing to go stale.
static BUILD: AtomicU64 = AtomicU64::new(1);

/// Where a page served for development listens unless asked to listen elsewhere. A page reloaded by hand, a bookmark and a second terminal all name the same address as long as it is free.
pub(crate) const DEFAULT_PORT: u16 = 8080;

/// Listens on `requested`, failing when it is taken. Without one, on `preferred` when free and otherwise on a port the system picks, so a port held by something else costs a different address rather than the session.
fn bind_dev_listener(requested: Option<u16>, preferred: u16) -> std::io::Result<TcpListener> {
    bind_dev_listener_with(requested, preferred, |port| {
        TcpListener::bind((Ipv4Addr::LOCALHOST, port))
    })
}

fn bind_dev_listener_with(
    requested: Option<u16>,
    preferred: u16,
    bind: impl Fn(u16) -> std::io::Result<TcpListener>,
) -> std::io::Result<TcpListener> {
    match requested {
        Some(port) => bind(port),
        None => bind(preferred).or_else(|_| bind(0)),
    }
}

/// `requested` is the port the user asked for, which the session fails on when it is taken. `opening` is the location the session was asked to show, which it prints the address of beside the site's own. `claim` is the server file the session names the site's address in once the first build is done, for the editor and a later `cargo telar preview` to find; it is removed when the session gives up.
pub(crate) fn run_web_dev(
    cargo_args: Vec<String>,
    config: TelarSection,
    requested: Option<u16>,
    renderer: Option<WebRenderer>,
    opening: Option<&str>,
    claim: Option<ServerClaim>,
) -> ! {
    let listener = match bind_dev_listener(requested, DEFAULT_PORT) {
        Ok(listener) => listener,
        Err(e) => {
            let port = requested.unwrap_or(DEFAULT_PORT);
            eprintln!("[cargo-telar] could not listen on port {port}: {e}");
            drop(claim);
            std::process::exit(1);
        }
    };
    let port = listener.local_addr().map_or(0, |at| at.port());
    if requested.is_none() && port != DEFAULT_PORT {
        eprintln!("[cargo-telar] Port {DEFAULT_PORT} is taken; serving on port {port} instead.");
    }

    let dist = match build_web_bundle(cargo_args.clone(), config.clone(), false, renderer, false) {
        Ok(dist) => dist,
        Err(e) => {
            eprintln!("[cargo-telar] {e}");
            drop(claim);
            std::process::exit(1);
        }
    };
    report_reload_poll(&dist);

    let base = config.web.base_path();
    let serve_from = dist.clone();
    let served_base = base.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let root = serve_from.clone();
            let base = served_base.clone();
            // One thread per request: a page pulls a handful of files and then holds a poll open, and a sequential server would make the poll block the next reload's fetches.
            std::thread::spawn(move || serve(stream, &root, &base));
        }
    });

    eprintln!("[cargo-telar] Serving http://localhost:{port}{base}");
    if let Some(claim) = &claim {
        claim.serve(&format!("http://localhost:{port}{base}"));
    }
    if let Some(location) = opening {
        eprintln!(
            "[cargo-telar] Open http://localhost:{port}{}",
            address_under(&base, location)
        );
    }

    watch_and_rebuild(cargo_args, config, &dist, renderer)
}

fn watch_and_rebuild(
    cargo_args: Vec<String>,
    config: TelarSection,
    dist: &Path,
    renderer: Option<WebRenderer>,
) -> ! {
    let watched = match PageWatch::collect(&cargo_args, &config) {
        Ok(watched) => watched,
        Err(e) => {
            eprintln!("[cargo-telar] error: {e}");
            idle();
        }
    };
    let changes = match ChangeFeed::start(watched) {
        Ok(changes) => changes,
        Err(e) => {
            eprintln!("[cargo-telar] could not watch for changes: {e}");
            idle();
        }
    };

    loop {
        changes.wait();
        changes.settle(SETTLE);

        eprintln!("[cargo-telar] Rebuilding...");
        // As in the desktop loop: without it an edited asset compiles against the artifact baked at startup, which fails its hash check.
        super::bake::bake_workspace();
        super::transpile::transpile_workspace();
        match build_web_bundle(cargo_args.clone(), config.clone(), false, renderer, false) {
            Ok(_) => {
                report_reload_poll(dist);
                BUILD.fetch_add(1, Ordering::Relaxed);
                eprintln!("[cargo-telar] Reloaded.");
            }
            Err(e) => eprintln!("[cargo-telar] {e}"),
        }
    }
}

/// Everything an edit that should rebuild the page can come from: what the desktop loop watches — every member's sources, assets and catalogs, and every `telar.toml` — and the page's own template and public files.
struct PageWatch {
    members: WatchSet,
    template: PathBuf,
    public: PathBuf,
}

impl PageWatch {
    fn collect(
        cargo_args: &[String],
        config: &TelarSection,
    ) -> Result<Self, telar_project::ManifestError> {
        let package_root = find_package_dir(cargo_args);
        Ok(Self::new(
            WatchSet::collect(&resolve_package(cargo_args).workspace_root)?,
            &config.web.template_path(&package_root).0,
            &config.web.public_dir(&package_root).0,
        ))
    }

    /// Canonical for the reason [`WatchSet::collect`] gives, through the template's directory, since the template may not exist yet.
    fn new(members: WatchSet, template: &Path, public: &Path) -> Self {
        let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let template = match (template.parent(), template.file_name()) {
            (Some(dir), Some(name)) => canonical(dir).join(name),
            _ => template.to_path_buf(),
        };
        Self {
            members,
            template,
            public: canonical(public),
        }
    }
}

impl WatchScope for PageWatch {
    /// The template through its directory, because an editor that saves by replacing the file would end a watch on the file itself.
    fn watches(&self) -> Vec<(PathBuf, RecursiveMode)> {
        let mut watches = self.members.watches();
        if let Some(dir) = self.template.parent().filter(|dir| dir.is_dir()) {
            watches.push((dir.to_path_buf(), RecursiveMode::NonRecursive));
        }
        if self.public.is_dir() {
            watches.push((self.public.clone(), RecursiveMode::Recursive));
        }
        watches
    }

    /// Any edit to the template or a public file counts, whatever its kind of file: the page serves them as they are.
    fn wants(&self, event: &notify::Event) -> bool {
        if self.members.wants(event) {
            return true;
        }
        matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        ) && event
            .paths
            .iter()
            .any(|path| *path == self.template || path.starts_with(&self.public))
    }
}

fn report_reload_poll(dist: &Path) {
    if let Err(e) = inject_reload_poll(dist) {
        eprintln!(
            "[cargo-telar] warning: live reload is off until the next rebuild: {e}. Reload the page by hand to see changes."
        );
    }
}

/// Adds the reload poll to the served page.
///
/// Added here rather than baked into the page the build writes, so what `cargo telar build` produces is the page that ships — a dev-only script has no business in it.
fn inject_reload_poll(dist: &Path) -> Result<(), String> {
    let page = dist.join("index.html");
    let html = std::fs::read_to_string(&page)
        .map_err(|e| format!("could not read {}: {e}", page.display()))?;
    if html.contains(RELOAD_MARKER) {
        return Ok(());
    }
    std::fs::write(&page, with_reload_poll(&html))
        .map_err(|e| format!("could not write {}: {e}", page.display()))
}

/// `html` with the reload poll before its last `</body>`, in whatever case it is spelled, or at the end of the document when it has none: a browser runs a script found after the body all the same.
fn with_reload_poll(html: &str) -> String {
    let at = body_end_tag(html).unwrap_or(html.len());
    format!("{}{RELOAD_SCRIPT}{}", &html[..at], &html[at..])
}

fn body_end_tag(html: &str) -> Option<usize> {
    const TAG: &str = "</body";
    let lowered = html.to_ascii_lowercase();
    lowered.rmatch_indices(TAG).map(|(at, _)| at).find(|&at| {
        lowered[at + TAG.len()..]
            .starts_with(|next: char| next == '>' || next.is_ascii_whitespace())
    })
}

const RELOAD_MARKER: &str = "telar-dev-reload";
const RELOAD_SCRIPT: &str = r#"    <script id="telar-dev-reload">
      // Development only: polls the build counter and reloads when it moves.
      (async () => {
        let seen = null;
        for (;;) {
          try {
            const build = await (await fetch('/telar-build', { cache: 'no-store' })).text();
            if (seen !== null && build !== seen) location.reload();
            seen = build;
          } catch {}
          await new Promise((r) => setTimeout(r, 700));
        }
      })();
      // The editor's preview panel frames the page and moves it from preview to preview by message, as a link would, rather than reloading it.
      addEventListener('message', (event) => {
        const goto = event.source === parent && parent !== window && event.data && event.data.telarGoto;
        if (typeof goto !== 'string') return;
        const to = new URL(goto, location.href);
        if (to.origin !== location.origin || to.href === location.href) return;
        history.pushState(null, '', to);
        dispatchEvent(new PopStateEvent('popstate', { state: null }));
      });
    </script>
"#;

fn serve(mut stream: TcpStream, root: &Path, base: &str) {
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(stream) => stream,
        Err(_) => return,
    });
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let mut request = request_line.split_whitespace();
    let method = request.next().unwrap_or("GET");
    let target = request.next().unwrap_or("/");
    let headers = read_headers(&mut reader);

    // A page on any other origin can resolve its own name to 127.0.0.1 and read this server as same-origin; the name it asks for still gives it away.
    if !headers.host.as_deref().is_some_and(names_this_machine) {
        respond(&mut stream, "403 Forbidden", &[], b"", false);
        return;
    }

    if target == "/telar-build" {
        let build = BUILD.load(Ordering::Relaxed).to_string();
        respond(
            &mut stream,
            "200 OK",
            &[("content-type", "text/plain".to_string())],
            build.as_bytes(),
            false,
        );
        return;
    }

    let head_only = match method {
        "GET" => false,
        "HEAD" => true,
        _ => {
            respond(
                &mut stream,
                "405 Method Not Allowed",
                &[("allow", "GET, HEAD".to_string())],
                b"",
                false,
            );
            return;
        }
    };
    if base != "/" && target.split(['?', '#']).next() == Some("/") {
        respond(
            &mut stream,
            "302 Found",
            &[("location", base.to_string())],
            b"",
            head_only,
        );
        return;
    }
    let Some((mut file, path, len)) = served_path(root, base, target, headers.accepts_html)
        .and_then(|path| {
            let file = std::fs::File::open(&path).ok()?;
            let len = file.metadata().ok()?.len();
            Some((file, path, len))
        })
    else {
        respond(&mut stream, "404 Not Found", &[], b"", false);
        return;
    };
    let mime = media_type(&path);

    match byte_range(headers.range.as_deref(), len) {
        ByteRange::Unsatisfiable => respond(
            &mut stream,
            "416 Range Not Satisfiable",
            &[("content-range", format!("bytes */{len}"))],
            b"",
            false,
        ),
        ByteRange::Slice { start, end } => {
            let mut body = vec![0; (end - start + 1) as usize];
            if file.seek(SeekFrom::Start(start)).is_err() || file.read_exact(&mut body).is_err() {
                respond(&mut stream, "500 Internal Server Error", &[], b"", false);
                return;
            }
            respond(
                &mut stream,
                "206 Partial Content",
                &[
                    ("content-type", mime.to_string()),
                    ("content-range", format!("bytes {start}-{end}/{len}")),
                    ("accept-ranges", "bytes".to_string()),
                ],
                &body,
                head_only,
            );
        }
        ByteRange::Whole => {
            let mut body = Vec::with_capacity(len as usize);
            if file.read_to_end(&mut body).is_err() {
                respond(&mut stream, "500 Internal Server Error", &[], b"", false);
                return;
            }
            let mut response_headers = vec![("content-type", mime.to_string())];
            // A debug module is tens of megabytes and compresses to a fraction of that. Uncompressed, the wait here is nothing like the one a real server puts a release build behind, and a measurement taken against this server reads the difference rather than the app.
            if headers.accepts_gzip && !head_only && compressible(&path) && body.len() >= GZIP_FLOOR
            {
                body = gzip(&body);
                response_headers.push(("content-encoding", "gzip".to_string()));
            } else {
                response_headers.push(("accept-ranges", "bytes".to_string()));
            }
            response_headers.push(("vary", "accept-encoding".to_string()));
            respond(&mut stream, "200 OK", &response_headers, &body, head_only);
        }
    }
}

/// Writes a whole response. `content-length` is always the body's, and a `HEAD` gets the headers a `GET` would without the bytes.
fn respond(
    stream: &mut TcpStream,
    status: &str,
    headers: &[(&str, String)],
    body: &[u8],
    head_only: bool,
) {
    let mut head = format!("HTTP/1.1 {status}\r\ncontent-length: {}\r\n", body.len());
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("cache-control: no-store\r\n\r\n");
    let _ = stream.write_all(head.as_bytes());
    if !head_only {
        let _ = stream.write_all(body);
    }
}

/// Below this a compressed body is the same size or larger, and the round trip through the encoder buys nothing.
const GZIP_FLOOR: usize = 1024;

fn gzip(body: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    if encoder.write_all(body).is_err() {
        return body.to_vec();
    }
    encoder.finish().unwrap_or_else(|_| body.to_vec())
}

/// The request headers this server acts on.
#[derive(Debug, Default, PartialEq, Eq)]
struct RequestHeaders {
    accepts_gzip: bool,
    /// Whether the browser asked for a page, as it does navigating to an address or reloading one.
    accepts_html: bool,
    range: Option<String>,
    host: Option<String>,
}

/// Whether a `Host` header names the loopback interface the server listens on, with or without a port.
fn names_this_machine(host: &str) -> bool {
    let (name, port) = match host.rsplit_once(':') {
        Some((name, port)) if !port.contains(']') => (name, Some(port)),
        _ => (host, None),
    };
    let numeric_port =
        port.is_none_or(|port| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()));
    numeric_port
        && ["localhost", "127.0.0.1", "[::1]"]
            .iter()
            .any(|local| name.eq_ignore_ascii_case(local))
}

/// The headers following the request line.
///
/// The rest of the request is read either way: what is left unread in the socket when the response goes out is what the browser sees as a connection reset.
fn read_headers(reader: &mut impl BufRead) -> RequestHeaders {
    let mut headers = RequestHeaders::default();
    let mut line = String::new();
    while reader.read_line(&mut line).is_ok_and(|read| read > 0) {
        if line.trim_end().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim();
            if name.eq_ignore_ascii_case("accept-encoding") {
                headers.accepts_gzip = value.to_ascii_lowercase().contains("gzip");
            } else if name.eq_ignore_ascii_case("accept") {
                headers.accepts_html = value.to_ascii_lowercase().contains("text/html");
            } else if name.eq_ignore_ascii_case("range") {
                headers.range = Some(value.trim().to_string());
            } else if name.eq_ignore_ascii_case("host") {
                headers.host = Some(value.trim().to_string());
            }
        }
        line.clear();
    }
    headers
}

/// What a `Range` header asks of a file `len` bytes long.
#[derive(Debug, PartialEq, Eq)]
enum ByteRange {
    /// No range, or one this server ignores, which HTTP allows: a range in another unit, several ranges, or one that does not parse.
    Whole,
    /// Bytes `start..=end`, already clamped to the file.
    Slice { start: u64, end: u64 },
    /// A range that starts past the end of the file.
    Unsatisfiable,
}

fn byte_range(header: Option<&str>, len: u64) -> ByteRange {
    let Some((unit, spec)) = header.and_then(|header| header.split_once('=')) else {
        return ByteRange::Whole;
    };
    if !unit.trim().eq_ignore_ascii_case("bytes") || spec.contains(',') {
        return ByteRange::Whole;
    }
    let Some((first, last)) = spec.split_once('-') else {
        return ByteRange::Whole;
    };
    let (first, last) = (first.trim(), last.trim());
    if first.is_empty() {
        let Some(suffix) = digits(last) else {
            return ByteRange::Whole;
        };
        if suffix == 0 || len == 0 {
            return ByteRange::Unsatisfiable;
        }
        return ByteRange::Slice {
            start: len.saturating_sub(suffix),
            end: len - 1,
        };
    }
    let Some(start) = digits(first) else {
        return ByteRange::Whole;
    };
    let end = match last {
        "" => None,
        last => match digits(last) {
            Some(end) if end >= start => Some(end),
            _ => return ByteRange::Whole,
        },
    };
    if start >= len {
        return ByteRange::Unsatisfiable;
    }
    ByteRange::Slice {
        start,
        end: end.map_or(len - 1, |end| end.min(len - 1)),
    }
}

fn digits(text: &str) -> Option<u64> {
    match !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        true => text.parse().ok(),
        false => None,
    }
}

/// The file a request target under `base` names, or `None` for one outside `base` or one that tries to leave the directory. A directory, the root included, serves its `index.html`.
fn resolve(root: &Path, base: &str, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or("/");
    let decoded = percent_decode(path)?;
    let under_base = decoded.strip_prefix(base.trim_end_matches('/'))?;
    if !under_base.is_empty() && !under_base.starts_with('/') {
        return None;
    }
    let relative = under_base.trim_start_matches('/');
    let escapes = relative
        .split('/')
        .any(|part| part == ".." || part.contains([':', '\\', '\0']));
    if escapes {
        return None;
    }
    let file = root.join(relative);
    if relative.is_empty() || relative.ends_with('/') || file.is_dir() {
        return Some(file.join("index.html"));
    }
    Some(file)
}

/// The file to answer `target` with: the one [`resolve`] names, or the site's page for a page asked for under `base` that no file is. The app's addresses are paths of its own, so reloading one, or following a link to one, has to load the app that reads it.
fn served_path(root: &Path, base: &str, target: &str, page_asked: bool) -> Option<PathBuf> {
    let path = resolve(root, base, target)?;
    match page_asked && !path.is_file() {
        true => Some(root.join("index.html")),
        false => Some(path),
    }
}

/// `location`, an app location written from the root, as the address of the site served under `base`.
pub(crate) fn address_under(base: &str, location: &str) -> String {
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        location.trim_start_matches('/')
    )
}

/// `%XX` escapes decoded to the bytes they stand for, or `None` when an escape is malformed or the result is not UTF-8.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            decoded.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

#[cfg(test)]
#[path = "web_dev_test.rs"]
mod tests;
