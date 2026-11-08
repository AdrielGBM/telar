use super::*;

#[test]
fn every_format_a_web_build_ships_has_its_own_media_type() {
    for (file, expected) in [
        ("index.html", "text/html; charset=utf-8"),
        ("app-0123456789ab.js", "text/javascript; charset=utf-8"),
        ("app_bg-0123456789ab.wasm", "application/wasm"),
        ("site.css", "text/css; charset=utf-8"),
        ("asset-manifest.json", "application/json"),
        ("site.webmanifest", "application/manifest+json"),
        ("logo.svg", "image/svg+xml"),
        ("photo.png", "image/png"),
        ("photo.jpg", "image/jpeg"),
        ("photo.jpeg", "image/jpeg"),
        ("photo.webp", "image/webp"),
        ("photo.avif", "image/avif"),
        ("favicon.ico", "image/x-icon"),
        ("fonts/Inter.woff2", "font/woff2"),
        ("fonts/Inter.ttf", "font/ttf"),
        ("fonts/Inter.otf", "font/otf"),
        ("clip.mp4", "video/mp4"),
        ("clip.webm", "video/webm"),
        ("robots.txt", "text/plain; charset=utf-8"),
        ("sitemap.xml", "application/xml"),
        ("cv.pdf", "application/pdf"),
    ] {
        assert_eq!(media_type(Path::new(file)), expected, "{file}");
    }
}

#[test]
fn the_extension_is_matched_without_regard_to_case() {
    assert_eq!(media_type(Path::new("PHOTO.JPG")), "image/jpeg");
    assert_eq!(media_type(Path::new("Clip.WebM")), "video/webm");
}

#[test]
fn an_unknown_or_missing_extension_is_opaque_bytes() {
    assert_eq!(
        media_type(Path::new("data.bin")),
        "application/octet-stream"
    );
    assert_eq!(media_type(Path::new("LICENSE")), "application/octet-stream");
}

#[test]
fn only_formats_that_are_not_already_compressed_are_worth_compressing() {
    for file in [
        "index.html",
        "app.js",
        "app_bg.wasm",
        "site.css",
        "logo.svg",
        "a.ttf",
        "robots.txt",
    ] {
        assert!(compressible(Path::new(file)), "{file}");
    }
    for file in [
        "photo.png",
        "photo.webp",
        "a.woff2",
        "clip.mp4",
        "cv.pdf",
        "data.bin",
    ] {
        assert!(!compressible(Path::new(file)), "{file}");
    }
}
