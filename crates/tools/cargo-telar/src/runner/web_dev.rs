//! `cargo telar dev --target web`: build, serve, rebuild on change, and tell the page to reload.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use notify::{Config as NotifyConfig, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use super::cli::WebRenderer;
use super::config::TelarConfig;
use super::package::build_web_bundle;

/// How long to let a burst of file events settle before rebuilding. An editor writing a file produces several, and a save that touches a whole directory produces one per file.
const SETTLE: Duration = Duration::from_millis(150);

/// Bumped by every successful rebuild. The page polls it and reloads when it moves, which is the whole live-reload protocol: no websocket, no client library, and nothing to go stale.
static BUILD: AtomicU64 = AtomicU64::new(1);

pub(crate) fn run_web_dev(
    cargo_args: Vec<String>,
    config: TelarConfig,
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
    config: TelarConfig,
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
    let mut request = String::new();
    if reader.read_line(&mut request).is_err() {
        return;
    }
    let path = request.split_whitespace().nth(1).unwrap_or("/");
    let accepts_gzip = accepts_gzip(&mut reader);

    if path == "/telar-build" {
        let build = BUILD.load(Ordering::Relaxed).to_string();
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: {}\r\ncache-control: no-store\r\n\r\n{build}",
            build.len()
        );
        return;
    }

    let Some(file) = resolve(root, path) else {
        let _ = write!(
            stream,
            "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n"
        );
        return;
    };
    let Ok(body) = std::fs::read(&file) else {
        let _ = write!(
            stream,
            "HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\n\r\n"
        );
        return;
    };
    let mime = match file.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        // Wrong here and the browser falls back to a slower non-streaming instantiation, silently.
        Some("wasm") => "application/wasm",
        _ => "application/octet-stream",
    };
    // A debug module is tens of megabytes and compresses to a fraction of that. Uncompressed, the wait here is nothing like the one a real server puts a release build behind, and a measurement taken against this server reads the difference rather than the app.
    let (body, encoding) = match accepts_gzip && body.len() >= GZIP_FLOOR {
        true => (gzip(&body), "content-encoding: gzip\r\n"),
        false => (body, ""),
    };
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\ncontent-type: {mime}\r\ncontent-length: {}\r\n{encoding}vary: accept-encoding\r\ncache-control: no-store\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(&body);
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

/// Whether the client said it takes gzip, from the headers following the request line.
///
/// The rest of the request is read either way: what is left unread in the socket when the response goes out is what the browser sees as a connection reset.
fn accepts_gzip(reader: &mut BufReader<TcpStream>) -> bool {
    let mut accepts = false;
    let mut line = String::new();
    while reader.read_line(&mut line).is_ok_and(|read| read > 0) {
        if line.trim_end().is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.trim().eq_ignore_ascii_case("accept-encoding")
        {
            accepts = value.to_ascii_lowercase().contains("gzip");
        }
        line.clear();
    }
    accepts
}

/// The file a request path names, or `None` for anything that tries to leave the directory.
fn resolve(root: &Path, path: &str) -> Option<PathBuf> {
    let path = path.split('?').next().unwrap_or("/");
    let relative = path.trim_start_matches('/');
    let relative = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    if relative.split('/').any(|part| part == "..") {
        return None;
    }
    Some(root.join(relative))
}
