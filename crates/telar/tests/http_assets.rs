//! Integration test for `HttpAssetSource`: the seam, the transport and the disk cache, end to end.
//!
//! The module's unit tests cover the cache-name rule in isolation. What this proves is the part that only shows up wired together — that a read returns `Loading` without blocking, that the worker's answer reaches the signal through `drain_tasks` on this thread, and that the second run resolves from disk with no server listening at all. A source that only ever spun would pass every unit test in the file.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use telar::async_assets::HttpAssetSource;
use telar::{AssetSource, AssetState, drain_tasks};

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M2 2 H22 V22 H2 Z"/></svg>"#;

/// A one-endpoint HTTP server on a loopback port, counting what it served so a cache hit is distinguishable from a second download rather than merely plausible.
struct Server {
    port: u16,
    hits: Arc<AtomicUsize>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().unwrap().port();
        let hits = Arc::new(AtomicUsize::new(0));
        let served = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut buf = [0u8; 1024];
                let _ = stream.read(&mut buf);
                let found = String::from_utf8_lossy(&buf).contains("/known.svg");
                let response = if found {
                    served.fetch_add(1, Ordering::Relaxed);
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: image/svg+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{SVG}",
                        SVG.len()
                    )
                } else {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_string()
                };
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { port, hits }
    }

    fn template(&self) -> String {
        format!("http://127.0.0.1:{}/{{name}}.svg", self.port)
    }
}

/// Drains delivered task values until `id` settles, or gives up. Stands in for the runner's per-frame `drain_tasks`, which is the only thing this test needs from a running app.
fn settle(
    source: &HttpAssetSource,
    id: &str,
    timeout: Duration,
) -> AssetState<Arc<telar::SvgData>> {
    let deadline = Instant::now() + timeout;
    loop {
        drain_tasks();
        let state = source.svg(id).get();
        if !matches!(state, AssetState::Loading) || Instant::now() > deadline {
            return state;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// `SvgData` has no `Debug`, and a failure message that says which of the three states was reached is the whole point of asserting on it.
fn name_of(state: &AssetState<Arc<telar::SvgData>>) -> &'static str {
    match state {
        AssetState::Loading => "Loading",
        AssetState::Ready(_) => "Ready",
        AssetState::Failed => "Failed",
    }
}

fn cache_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("telar-http-assets-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_fetched_asset_lands_in_its_signal_and_then_in_the_cache() {
    let server = Server::start();
    let dir = cache_dir("fetched");
    let source = HttpAssetSource::new(server.template()).cached_in(&dir);

    assert!(
        matches!(source.svg("known").get(), AssetState::Loading),
        "the first read must return without waiting for the network"
    );

    let state = settle(&source, "known", Duration::from_secs(10));
    assert!(
        state.is_ready(),
        "asset did not resolve: {}",
        name_of(&state)
    );
    assert_eq!(server.hits.load(Ordering::Relaxed), 1);
    assert!(dir.join("known.svg").exists(), "download was not cached");

    // A second source over the same directory answers from disk: the server count stays where it was.
    let offline = HttpAssetSource::new(server.template()).cached_in(&dir);
    assert!(settle(&offline, "known", Duration::from_secs(10)).is_ready());
    assert_eq!(
        server.hits.load(Ordering::Relaxed),
        1,
        "the cached asset was downloaded again"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_asset_the_endpoint_does_not_have_settles_on_failed() {
    let server = Server::start();
    let dir = cache_dir("missing");
    // Two attempts, back to back: the retry path runs, and the test does not wait the default 4s for it.
    let source = HttpAssetSource::new(server.template())
        .cached_in(&dir)
        .with_retry(2, Duration::from_millis(20));

    let state = settle(&source, "absent", Duration::from_secs(10));
    assert!(
        matches!(state, AssetState::Failed),
        "a missing asset must settle rather than spin forever, got {}",
        name_of(&state)
    );
    assert!(
        !dir.join("absent.svg").exists(),
        "a failed fetch must not be cached"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Reading the same id twice must not queue it twice, however many widgets ask.
#[test]
fn repeated_reads_share_one_request() {
    let server = Server::start();
    let dir = cache_dir("shared");
    let source = HttpAssetSource::new(server.template()).cached_in(&dir);

    for _ in 0..5 {
        let _ = source.svg("known").get();
    }
    assert!(settle(&source, "known", Duration::from_secs(10)).is_ready());
    assert_eq!(server.hits.load(Ordering::Relaxed), 1);

    let _ = std::fs::remove_dir_all(&dir);
}
