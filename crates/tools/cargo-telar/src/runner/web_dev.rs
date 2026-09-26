//! `cargo telar dev --target web`: build, serve, rebuild on change, and tell the page to reload.

use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use super::cli::WebRenderer;
use super::config::{TelarSection, find_package_dir};
use super::package::{build_web_bundle, compressible, media_type};

/// How long to let a burst of file events settle before rebuilding. An editor writing a file produces several, and a save that touches a whole directory produces one per file.
const SETTLE: Duration = Duration::from_millis(150);

/// Bumped by every successful rebuild. The page polls it and reloads when it moves, which is the whole live-reload protocol: no websocket, no client library, and nothing to go stale.
static BUILD: AtomicU64 = AtomicU64::new(1);

pub(crate) fn run_web_dev(
    cargo_args: Vec<String>,
    config: TelarSection,
    port: u16,
    renderer: Option<WebRenderer>,
) -> ! {
    let dist = match build_web_bundle(cargo_args.clone(), config.clone(), false, renderer) {
        Ok(dist) => dist,
        Err(e) => {
            eprintln!("[cargo-telar] {e}");
            std::process::exit(1);
        }
    };
    inject_reload_poll(&dist);

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => listener,
        Err(e) => {
            eprintln!("[cargo-telar] could not listen on port {port}: {e}");
            std::process::exit(1);
        }
    };
    eprintln!("[cargo-telar] Serving http://localhost:{port}/");

    let serve_from = dist.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let root = serve_from.clone();
            // One thread per request: a page pulls a handful of files and then holds a poll open, and a sequential server would make the poll block the next reload's fetches.
            std::thread::spawn(move || serve(stream, &root));
        }
    });

    watch_and_rebuild(cargo_args, config, &dist, renderer)
}

fn watch_and_rebuild(
    cargo_args: Vec<String>,
    config: TelarSection,
    dist: &Path,
    renderer: Option<WebRenderer>,
) -> ! {
    let (tx, rx) = mpsc::channel();
    let mut watcher = match RecommendedWatcher::new(tx, NotifyConfig::default()) {
        Ok(watcher) => watcher,
        Err(e) => {
            eprintln!("[cargo-telar] could not watch for changes: {e}");
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        }
    };
    let _ = watcher.watch(Path::new("src"), RecursiveMode::Recursive);
    let _ = watcher.watch(Path::new("crates"), RecursiveMode::Recursive);
    let _ = watcher.watch(Path::new("apps"), RecursiveMode::Recursive);
    let package_root = find_package_dir(&cargo_args);
    // The template's directory rather than the file: an editor that saves by replacing the file would end a watch on the file itself.
    if let Some(template_dir) = config.web.template_path(&package_root).0.parent() {
        let _ = watcher.watch(template_dir, RecursiveMode::NonRecursive);
    }
    let _ = watcher.watch(
        &config.web.public_dir(&package_root).0,
        RecursiveMode::Recursive,
    );

    loop {
        let Ok(event) = rx.recv() else {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        };
        if !matches!(
            event.as_ref().map(|e| &e.kind),
            Ok(EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_))
        ) {
            continue;
        }
        // Drain the rest of the burst rather than rebuilding once per file.
        while rx.recv_timeout(SETTLE).is_ok() {}

        eprintln!("[cargo-telar] Rebuilding...");
        match build_web_bundle(cargo_args.clone(), config.clone(), false, renderer) {
            Ok(_) => {
                inject_reload_poll(dist);
                BUILD.fetch_add(1, Ordering::Relaxed);
                eprintln!("[cargo-telar] Reloaded.");
            }
            Err(e) => eprintln!("[cargo-telar] {e}"),
        }
    }
}

/// Appends the reload poll to the served page.
///
/// Added here rather than baked into the page the build writes, so what `cargo telar build` produces is the page that ships — a dev-only script has no business in it.
fn inject_reload_poll(dist: &Path) {
    let page = dist.join("index.html");
    let Ok(html) = std::fs::read_to_string(&page) else {
        return;
    };
    if html.contains(RELOAD_MARKER) {
        return;
    }
    let injected = html.replace("</body>", &format!("{RELOAD_SCRIPT}</body>"));
    let _ = std::fs::write(&page, injected);
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
    </script>
"#;

fn serve(mut stream: TcpStream, root: &Path) {
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
    let Some((mut file, path, len)) = resolve(root, target).and_then(|path| {
        let file = std::fs::File::open(&path).ok()?;
        let len = file.metadata().ok()?.len();
        Some((file, path, len))
    }) else {
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
    range: Option<String>,
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
            } else if name.eq_ignore_ascii_case("range") {
                headers.range = Some(value.trim().to_string());
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

/// The file a request target names, or `None` for anything that tries to leave the directory. A directory, the root included, serves its `index.html`.
fn resolve(root: &Path, target: &str) -> Option<PathBuf> {
    let path = target.split(['?', '#']).next().unwrap_or("/");
    let decoded = percent_decode(path)?;
    let relative = decoded.trim_start_matches('/');
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
