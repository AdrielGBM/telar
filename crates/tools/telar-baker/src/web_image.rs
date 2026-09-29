//! The files a browser build ships for an `img src:"…"`, and the addresses the app is baked with for them.
//!
//! One function for both sides: the baker writes the addresses into the app, the packager writes the files at them, and neither could agree with the other if each named them itself.

/// The directory of the output the files go in.
pub const WEB_IMAGES_DIR: &str = "images";

/// One file, at the path inside the output a baked app fetches it from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebImageFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

/// Every file a browser build ships for one picture. See [`renderer_assets::WebImage`].
#[derive(Debug, Clone)]
pub struct WebImage {
    pub width: u32,
    pub height: u32,
    pub full: WebImageFile,
    /// `(width, file)`, narrowest first.
    pub copies: Vec<(u32, WebImageFile)>,
}

/// The files for the picture in `bytes`, named by the hash of those bytes: a changed picture is a new address, which is what lets a browser cache one forever.
pub fn web_image(bytes: &[u8]) -> Result<WebImage, String> {
    let web = renderer_assets::web_image(bytes).map_err(|e| e.to_string())?;
    let hash = telar_project::content_hash(bytes);
    let file = |suffix: String, file: renderer_assets::WebImageFile| WebImageFile {
        path: format!("{WEB_IMAGES_DIR}/{hash}{suffix}.{}", file.extension),
        bytes: file.bytes,
    };
    Ok(WebImage {
        width: web.width,
        height: web.height,
        full: file(String::new(), web.full),
        copies: web
            .copies
            .into_iter()
            .map(|(width, copy)| (width, file(format!("-{width}w"), copy)))
            .collect(),
    })
}

#[cfg(test)]
#[path = "web_image_test.rs"]
mod tests;
