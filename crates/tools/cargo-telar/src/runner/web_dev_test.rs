use super::*;

fn slice(start: u64, end: u64) -> ByteRange {
    ByteRange::Slice { start, end }
}

#[test]
fn no_range_header_is_the_whole_file() {
    assert_eq!(byte_range(None, 100), ByteRange::Whole);
}

#[test]
fn a_closed_range_is_served_as_asked() {
    assert_eq!(byte_range(Some("bytes=0-0"), 100), slice(0, 0));
    assert_eq!(byte_range(Some("bytes=10-19"), 100), slice(10, 19));
    assert_eq!(byte_range(Some("bytes= 10 - 19 "), 100), slice(10, 19));
}

#[test]
fn an_end_past_the_file_is_clamped_to_its_last_byte() {
    assert_eq!(byte_range(Some("bytes=90-500"), 100), slice(90, 99));
}

/// What a `<video>` sends first, to learn the length and start playing without the whole file.
#[test]
fn an_open_range_runs_to_the_end() {
    assert_eq!(byte_range(Some("bytes=0-"), 100), slice(0, 99));
    assert_eq!(byte_range(Some("bytes=40-"), 100), slice(40, 99));
}

#[test]
fn a_suffix_range_is_the_last_n_bytes() {
    assert_eq!(byte_range(Some("bytes=-10"), 100), slice(90, 99));
    assert_eq!(byte_range(Some("bytes=-500"), 100), slice(0, 99));
}

#[test]
fn the_unit_is_matched_without_regard_to_case() {
    assert_eq!(byte_range(Some("Bytes=0-9"), 100), slice(0, 9));
}

#[test]
fn a_range_past_the_end_cannot_be_satisfied() {
    assert_eq!(
        byte_range(Some("bytes=100-"), 100),
        ByteRange::Unsatisfiable
    );
    assert_eq!(
        byte_range(Some("bytes=200-300"), 100),
        ByteRange::Unsatisfiable
    );
    assert_eq!(byte_range(Some("bytes=-0"), 100), ByteRange::Unsatisfiable);
    assert_eq!(byte_range(Some("bytes=-5"), 0), ByteRange::Unsatisfiable);
}

/// HTTP lets a server answer any `Range` it does not support with the whole representation, which is always correct.
#[test]
fn a_range_this_server_does_not_take_is_ignored() {
    for header in [
        "items=0-9",
        "bytes=0-9,20-29",
        "bytes=9-0",
        "bytes=a-b",
        "bytes=+1-5",
        "bytes=-",
        "bytes=5",
        "bytes",
    ] {
        assert_eq!(byte_range(Some(header), 100), ByteRange::Whole, "{header}");
    }
}

#[test]
fn the_headers_this_server_acts_on_are_read_and_the_rest_consumed() {
    let request = "Host: localhost\r\nACCEPT-ENCODING: gzip, br\r\nAccept: text/html,application/xhtml+xml\r\nRange: bytes=0-\r\n\r\nleftover";
    let mut reader = std::io::Cursor::new(request.as_bytes());
    let headers = read_headers(&mut reader);
    assert_eq!(
        headers,
        RequestHeaders {
            accepts_gzip: true,
            accepts_html: true,
            range: Some("bytes=0-".to_string()),
        }
    );
    let mut rest = String::new();
    reader.read_to_string(&mut rest).unwrap();
    assert_eq!(rest, "leftover", "reading stops at the blank line");
}

fn site(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_web_dev_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("docs")).unwrap();
    root
}

#[test]
fn a_request_target_names_a_file_under_the_root() {
    let root = site("files");
    assert_eq!(resolve(&root, "/", "/app.js"), Some(root.join("app.js")));
    assert_eq!(
        resolve(&root, "/", "/app.js?v=2#top"),
        Some(root.join("app.js"))
    );
    assert_eq!(
        resolve(&root, "/", "/fonts/Inter%20Display.woff2"),
        Some(root.join("fonts/Inter Display.woff2"))
    );
    assert_eq!(
        resolve(&root, "/", "/.well-known/security.txt"),
        Some(root.join(".well-known/security.txt"))
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_directory_serves_its_index() {
    let root = site("index");
    assert_eq!(resolve(&root, "/", "/"), Some(root.join("index.html")));
    assert_eq!(
        resolve(&root, "/", "/es/"),
        Some(root.join("es/index.html"))
    );
    assert_eq!(
        resolve(&root, "/", "/docs"),
        Some(root.join("docs/index.html"))
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_site_under_a_base_serves_only_below_it() {
    let root = site("base");
    assert_eq!(
        resolve(&root, "/docs/", "/docs/app.js"),
        Some(root.join("app.js"))
    );
    assert_eq!(
        resolve(&root, "/docs/", "/docs/"),
        Some(root.join("index.html"))
    );
    assert_eq!(
        resolve(&root, "/docs/", "/docs"),
        Some(root.join("index.html"))
    );
    assert_eq!(resolve(&root, "/docs/", "/app.js"), None);
    assert_eq!(resolve(&root, "/docs/", "/docsy/app.js"), None);
    let _ = std::fs::remove_dir_all(&root);
}

/// A reload at `/preview/<id>` used to find no file and answer 404, so a page whose state lives in its address lost it on every rebuild.
#[test]
fn a_page_asked_for_at_an_app_address_is_the_site_s_page() {
    let root = site("pages");
    std::fs::write(root.join("app.js"), "").unwrap();
    assert_eq!(
        served_path(&root, "/", "/preview/demo--card--a?args=n:2", true),
        Some(root.join("index.html"))
    );
    assert_eq!(
        served_path(&root, "/docs/", "/docs/matrix/demo--card--a", true),
        Some(root.join("index.html"))
    );
    assert_eq!(
        served_path(&root, "/", "/app.js", true),
        Some(root.join("app.js"))
    );
    assert_eq!(
        served_path(&root, "/", "/missing.js", false),
        Some(root.join("missing.js")),
        "a script that is not there is still not found"
    );
    assert_eq!(served_path(&root, "/docs/", "/other/page", true), None);
    assert_eq!(served_path(&root, "/", "/../secret", true), None);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_location_is_addressed_under_the_site_s_base() {
    assert_eq!(
        address_under("/", "/preview/demo--card--a"),
        "/preview/demo--card--a"
    );
    assert_eq!(
        address_under("/docs/", "/matrix/demo--card--a?axes=mode"),
        "/docs/matrix/demo--card--a?axes=mode"
    );
}

#[test]
fn a_target_that_would_leave_the_root_is_refused() {
    let root = Path::new("/srv/site");
    for target in [
        "/../secret",
        "/a/../../secret",
        "/%2e%2e/secret",
        "/..%2fsecret",
        "/C:/Windows",
        "/a%5c..%5csecret",
        "/%00",
        "/%zz",
        "/%e",
    ] {
        assert_eq!(resolve(root, "/", target), None, "{target}");
    }
}

#[test]
fn percent_escapes_decode_to_utf8() {
    assert_eq!(percent_decode("caf%C3%A9").as_deref(), Some("café"));
    assert_eq!(percent_decode("a+b").as_deref(), Some("a+b"));
    assert_eq!(percent_decode("%ff"), None, "not UTF-8");
}

fn edited(path: &Path) -> notify::Event {
    notify::Event {
        kind: EventKind::Modify(notify::event::ModifyKind::Data(
            notify::event::DataChange::Content,
        )),
        paths: vec![path.to_path_buf()],
        attrs: Default::default(),
    }
}

/// The loop used to watch `src/`, `crates/` and `apps/` under wherever it was started: nothing under `plugins/`, or anywhere a workspace keeps its members, rebuilt the page.
#[test]
fn the_page_rebuilds_on_what_the_desktop_loop_watches_and_on_its_own_files() {
    let root = std::env::temp_dir().join(format!("telar_web_dev_watch_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["src", "plugins/tool/src", "public/css", "target/debug"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"site\"\n\n[workspace]\nmembers = [\"plugins/tool\"]\n",
    )
    .unwrap();
    std::fs::write(
        root.join("plugins/tool/Cargo.toml"),
        "[package]\nname = \"tool\"\n",
    )
    .unwrap();
    let root = root.canonicalize().unwrap();
    let watch = PageWatch::new(
        WatchSet::collect(&root),
        &root.join("index.html"),
        &root.join("public"),
    );

    for rebuilds in [
        "src/lib.rs",
        "plugins/tool/src/lib.rs",
        "plugins/tool/telar.toml",
        "index.html",
        "public/css/site.css",
    ] {
        assert!(watch.wants(&edited(&root.join(rebuilds))), "{rebuilds}");
    }
    for ignored in ["target/debug/out.rs", "Cargo.lock", "notes.txt"] {
        assert!(!watch.wants(&edited(&root.join(ignored))), "{ignored}");
    }
    let watched: Vec<PathBuf> = watch.watches().into_iter().map(|(dir, _)| dir).collect();
    for dir in ["src", "plugins/tool/src", "public", ""] {
        assert!(watched.contains(&root.join(dir)), "{dir}: {watched:?}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

fn taken_port() -> (TcpListener, u16) {
    let holder = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = holder.local_addr().unwrap().port();
    (holder, port)
}

#[test]
fn the_preferred_port_is_used_when_it_is_free() {
    let (holder, port) = taken_port();
    drop(holder);
    let listener = bind_dev_listener(None, port).unwrap();
    assert_eq!(listener.local_addr().unwrap().port(), port);
}

#[test]
fn a_taken_default_port_falls_back_to_a_free_one() {
    let (_holder, port) = taken_port();
    let listener = bind_dev_listener(None, port).unwrap();
    let chosen = listener.local_addr().unwrap().port();
    assert_ne!(chosen, port);
    assert_ne!(chosen, 0);
}

#[test]
fn a_taken_port_the_user_asked_for_is_an_error() {
    let (_holder, port) = taken_port();
    assert!(bind_dev_listener(Some(port), 8080).is_err());
}

#[test]
fn a_free_port_the_user_asked_for_is_used() {
    let (holder, port) = taken_port();
    drop(holder);
    let listener = bind_dev_listener(Some(port), 8080).unwrap();
    assert_eq!(listener.local_addr().unwrap().port(), port);
}
