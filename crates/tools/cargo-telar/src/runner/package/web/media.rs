//! What a file in a browser build is, by its extension: the media type a server labels it with, and whether compressing it is worth anything.

use std::path::Path;

/// The `content-type` to serve `path` under, `application/octet-stream` for an extension this does not know.
pub(crate) fn media_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        // Wrong here and the browser falls back to a slower non-streaming instantiation, silently.
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "webmanifest" => "application/manifest+json",
        "txt" => "text/plain; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",
        "xml" => "application/xml",
        "vtt" => "text/vtt; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "apng" => "image/apng",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "ktx2" => "image/ktx2",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "ogg" | "oga" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        _ => "application/octet-stream",
    }
}

/// Whether a compressed copy of `path` would be meaningfully smaller: text, the module, and the formats that store their bytes raw. Images, video, audio and `woff`/`woff2` are compressed already, and a second pass over them costs time for nothing.
pub(crate) fn compressible(path: &Path) -> bool {
    let media = media_type(path);
    media.starts_with("text/")
        || matches!(
            media,
            "application/wasm"
                | "application/json"
                | "application/manifest+json"
                | "application/xml"
                | "image/svg+xml"
                | "image/x-icon"
                | "image/bmp"
                | "font/ttf"
                | "font/otf"
                | "model/gltf+json"
        )
}

#[cfg(test)]
#[path = "media_test.rs"]
mod tests;
