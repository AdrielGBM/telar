use std::io::Read;

use super::*;

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("telar_web_assets_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn hash_of(name: &str) -> &str {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    stem.rsplit_once('-').unwrap().1
}

#[test]
fn a_hashed_name_keeps_the_directory_and_the_extension() {
    let hashed = hashed_name("fonts/Inter.woff2", b"font");
    assert!(hashed.starts_with("fonts/Inter-"), "{hashed}");
    assert!(hashed.ends_with(".woff2"), "{hashed}");
    assert_eq!(hash_of(&hashed).len(), HASH_LEN);
    assert!(hash_of(&hashed).bytes().all(|b| b.is_ascii_hexdigit()));

    let module = hashed_name("app_bg.wasm", b"module");
    assert!(module.starts_with("app_bg-") && module.ends_with(".wasm"));
    let bare = hashed_name("LICENSE", b"text");
    assert!(bare.starts_with("LICENSE-") && !bare.contains('.'));
}

/// The whole point of a hashed name: the same bytes keep their URL across builds, and different bytes never share one.
#[test]
fn the_hash_follows_the_content_and_nothing_else() {
    assert_eq!(hashed_name("app.js", b"a"), hashed_name("app.js", b"a"));
    assert_ne!(hashed_name("app.js", b"a"), hashed_name("app.js", b"b"));
}

#[test]
fn emitted_files_are_written_under_their_hashed_names_and_listed_in_the_manifest() {
    let root = scratch("emit");
    let mut assets = Assets::new(&root);
    let font = assets.emit("fonts/Inter.woff2", b"font bytes").unwrap();
    std::fs::write(root.join("app.js"), b"glue").unwrap();
    let glue = assets.adopt("app.js").unwrap();

    assert_eq!(std::fs::read(root.join(&font)).unwrap(), b"font bytes");
    assert_eq!(std::fs::read(root.join(&glue)).unwrap(), b"glue");
    assert!(!root.join("app.js").exists(), "adopting moves the file");

    assets.write_manifest().unwrap();
    let manifest: BTreeMap<String, String> =
        serde_json::from_slice(&std::fs::read(root.join(MANIFEST_FILE)).unwrap()).unwrap();
    assert_eq!(
        manifest,
        BTreeMap::from([
            ("app.js".to_string(), glue),
            ("fonts/Inter.woff2".to_string(), font),
        ])
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_logical_path_emitted_twice_is_refused() {
    let root = scratch("twice");
    let mut assets = Assets::new(&root);
    assets.emit("a.css", b"one").unwrap();
    assert!(assets.emit("a.css", b"two").is_err());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_logical_path_that_leaves_the_output_is_refused() {
    let root = scratch("escape");
    let mut assets = Assets::new(&root);
    for logical in [
        "",
        "/abs.js",
        "../up.js",
        "a/../b.js",
        "a//b.js",
        "./a.js",
        "a\\b.js",
    ] {
        assert!(assets.emit(logical, b"x").is_err(), "{logical:?}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_public_directory_is_copied_verbatim_with_its_structure() {
    let root = scratch("public");
    let public = root.join("public");
    let out = root.join("out");
    std::fs::create_dir_all(public.join(".well-known")).unwrap();
    std::fs::create_dir_all(public.join("img/og")).unwrap();
    std::fs::write(public.join("robots.txt"), "User-agent: *\n").unwrap();
    std::fs::write(public.join(".well-known/security.txt"), "Contact: x\n").unwrap();
    std::fs::write(public.join("img/og/es.png"), b"png").unwrap();

    let copied = copy_public(&public, &out, &["index.html"]).unwrap();
    assert_eq!(copied, 3);
    assert_eq!(
        std::fs::read_to_string(out.join("robots.txt")).unwrap(),
        "User-agent: *\n"
    );
    assert!(out.join(".well-known/security.txt").is_file());
    assert_eq!(std::fs::read(out.join("img/og/es.png")).unwrap(), b"png");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_public_file_cannot_replace_what_the_build_writes() {
    let root = scratch("reserved");
    let public = root.join("public");
    let out = root.join("out");
    std::fs::create_dir_all(&public).unwrap();
    std::fs::write(public.join("index.html"), "<p>mine</p>").unwrap();
    let error = copy_public(&public, &out, &["index.html"]).unwrap_err();
    assert!(error.contains("index.html"), "{error}");

    std::fs::remove_file(public.join("index.html")).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(public.join("app-0123456789ab.js"), "x").unwrap();
    std::fs::write(out.join("app-0123456789ab.js"), "y").unwrap();
    assert!(
        copy_public(&public, &out, &[]).is_err(),
        "a hashed file already there"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn precompression_covers_text_in_every_directory_and_skips_what_is_already_compressed() {
    let root = scratch("precompress");
    let text = "body { color: red; }\n".repeat(200);
    std::fs::create_dir_all(root.join("css")).unwrap();
    std::fs::write(root.join("index.html"), &text).unwrap();
    std::fs::write(root.join("css/site.css"), &text).unwrap();
    std::fs::write(root.join("small.js"), "x").unwrap();
    std::fs::write(root.join("photo.png"), &text).unwrap();

    precompress(&root).unwrap();
    for compressed in [
        "index.html.br",
        "index.html.gz",
        "css/site.css.br",
        "css/site.css.gz",
    ] {
        assert!(root.join(compressed).is_file(), "{compressed}");
    }
    for skipped in ["small.js.br", "photo.png.br", "photo.png.gz"] {
        assert!(!root.join(skipped).exists(), "{skipped}");
    }

    let mut decoded = Vec::new();
    flate2::read::GzDecoder::new(std::fs::File::open(root.join("css/site.css.gz")).unwrap())
        .read_to_end(&mut decoded)
        .unwrap();
    assert_eq!(decoded, text.as_bytes());
    let _ = std::fs::remove_dir_all(&root);
}
