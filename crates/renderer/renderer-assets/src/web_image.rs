//! A bitmap as a browser build ships it: a file the page fetches, and smaller copies of it for small boxes.

use std::io::Cursor;

use image::{DynamicImage, ImageFormat, imageops::FilterType};

use crate::image::ImageError;

/// Below this width a copy saves too little to be worth another request's worth of choice.
const NARROWEST_COPY: u32 = 480;

/// JPEG quality for a copy: indistinguishable from the original at the size it is shown, at a fraction of its weight.
const COPY_QUALITY: u8 = 85;

/// One file of a [`WebImage`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebImageFile {
    /// The extension the file is served under, without the dot.
    pub extension: &'static str,
    pub bytes: Vec<u8>,
}

/// What a browser build ships for one bitmap.
#[derive(Debug, Clone)]
pub struct WebImage {
    pub width: u32,
    pub height: u32,
    /// The picture at its own size: the source file as it is when a browser reads its format, re-encoded as PNG when it does not.
    pub full: WebImageFile,
    /// Copies at half the width, then half again, while they stay at least 480 pixels wide, as `(width, file)`, narrowest first. Only a format this build can encode has copies, and only a copy smaller than the file above it is kept: a lossless copy of a lossy photograph can outweigh it.
    pub copies: Vec<(u32, WebImageFile)>,
}

/// Decodes `bytes` and works out every file a browser build ships for them. Deterministic: the same bytes give the same files, so the baker naming them and the packager writing them agree without talking.
pub fn web_image(bytes: &[u8]) -> Result<WebImage, ImageError> {
    let format = image::guess_format(bytes).map_err(ImageError::from_display)?;
    let decoded =
        image::load_from_memory_with_format(bytes, format).map_err(ImageError::from_display)?;
    let (width, height) = (decoded.width(), decoded.height());
    let full = match browser_extension(format) {
        Some(extension) => WebImageFile {
            extension,
            bytes: bytes.to_vec(),
        },
        None => encode(&decoded, ImageFormat::Png)?,
    };
    let copy_format = match format {
        ImageFormat::Jpeg => Some(ImageFormat::Jpeg),
        ImageFormat::WebP => Some(ImageFormat::WebP),
        ImageFormat::Gif | ImageFormat::Avif | ImageFormat::Ico => None,
        _ => Some(ImageFormat::Png),
    };
    let mut copies = Vec::new();
    if let Some(copy_format) = copy_format {
        let mut above = full.bytes.len();
        let mut copy_width = width / 2;
        while copy_width >= NARROWEST_COPY {
            let copy_height = ((height as u64 * copy_width as u64) / width as u64).max(1) as u32;
            let resized = decoded.resize_exact(copy_width, copy_height, FilterType::Lanczos3);
            let file = encode(&resized, copy_format)?;
            if file.bytes.len() >= above {
                break;
            }
            above = file.bytes.len();
            copies.push((copy_width, file));
            copy_width /= 2;
        }
    }
    copies.reverse();
    Ok(WebImage {
        width,
        height,
        full,
        copies,
    })
}

/// The extension a browser shows `format` under, or `None` for a format no browser reads.
fn browser_extension(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Png => Some("png"),
        ImageFormat::Jpeg => Some("jpg"),
        ImageFormat::Gif => Some("gif"),
        ImageFormat::WebP => Some("webp"),
        ImageFormat::Avif => Some("avif"),
        ImageFormat::Bmp => Some("bmp"),
        ImageFormat::Ico => Some("ico"),
        _ => None,
    }
}

fn encode(image: &DynamicImage, format: ImageFormat) -> Result<WebImageFile, ImageError> {
    let mut bytes = Vec::new();
    let extension = match format {
        ImageFormat::Jpeg => {
            let encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, COPY_QUALITY);
            image
                .to_rgb8()
                .write_with_encoder(encoder)
                .map_err(ImageError::from_display)?;
            "jpg"
        }
        ImageFormat::WebP => {
            image
                .to_rgba8()
                .write_to(&mut Cursor::new(&mut bytes), ImageFormat::WebP)
                .map_err(ImageError::from_display)?;
            "webp"
        }
        _ => {
            image
                .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
                .map_err(ImageError::from_display)?;
            "png"
        }
    };
    Ok(WebImageFile { extension, bytes })
}

#[cfg(test)]
#[path = "web_image_test.rs"]
mod tests;
