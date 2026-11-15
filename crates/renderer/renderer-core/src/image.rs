//! [`ImageData`]: premultiplied RGBA pixels addressed by content, plus the seam for a texture the application owns.

use wide::u32x4;
use xxhash_rust::xxh3::xxh3_64_with_seed;

/// A texture an application rendered itself, handed to Telar to place in the frame.
///
/// Deliberately opaque here: this crate is the vocabulary the CPU rasterizer shares with the GPU one, and naming a `wgpu` type would drag the whole GPU stack into builds that never touch one. The backend that can use the handle downcasts it; the one that cannot ignores the command.
///
/// Which means the trait is public for the backends, not for applications: a handle only draws through the backend that made it, so an implementation written outside one is recognised by nobody and its image comes out empty (with a warning). Applications build these with `telar::gpu::image`.
pub trait ExternalTexture: std::fmt::Debug + Send + Sync {
    fn as_any(&self) -> &dyn std::any::Any;
}

#[derive(Debug, Clone)]
enum ImageSource {
    Pixels(Vec<u8>),
    External(std::sync::Arc<dyn ExternalTexture>),
    Linked(std::sync::Arc<Linked>),
}

/// A picture that lives at an address instead of in the binary: what a web build ships for `img src:"…"`.
///
/// The page fetches it and a canvas loads it; only the intrinsic size travels with the app, which is what layout needs before a byte has arrived.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Linked {
    url: std::sync::Arc<str>,
    variants: std::sync::Arc<[(u32, std::sync::Arc<str>)]>,
}

impl Linked {
    /// `url` with its `variants`, refused unless the variants run narrowest first.
    pub(crate) fn ordered(
        url: std::sync::Arc<str>,
        variants: std::sync::Arc<[(u32, std::sync::Arc<str>)]>,
    ) -> Option<Self> {
        variants
            .is_sorted_by_key(|(width, _)| *width)
            .then_some(Self { url, variants })
    }

    /// The address of the picture at its full size, relative to the page.
    pub fn url(&self) -> &std::sync::Arc<str> {
        &self.url
    }

    /// Smaller copies of the same picture, as `(width in pixels, address)`, narrowest first: what a document offers as `srcset` so a small box does not download a large picture.
    pub fn variants(&self) -> &[(u32, std::sync::Arc<str>)] {
        &self.variants
    }
}

/// Bytes that are not the RGBA8 pixels of the image they were handed in for: `width * height * 4` of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageDataError {
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
}

impl std::fmt::Display for ImageDataError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} bytes are not the RGBA8 pixels of a {}x{} image",
            self.bytes, self.width, self.height
        )
    }
}

impl std::error::Error for ImageDataError {}

#[derive(Debug, Clone)]
/// A bitmap in premultiplied RGBA, addressed by the content hash of its pixels and dimensions.
pub struct ImageData {
    id: u64,
    source: ImageSource,
    width: u32,
    height: u32,
}

impl ImageData {
    /// Builds from straight-alpha RGBA8 `pixels`, refusing a buffer that is not `width * height * 4` bytes.
    pub fn new(mut pixels: Vec<u8>, width: u32, height: u32) -> Result<Self, ImageDataError> {
        check_fills(&pixels, width, height)?;
        premultiply_rgba(&mut pixels);
        Ok(Self::addressed(pixels, width, height))
    }

    /// Builds from bytes that are ALREADY premultiplied (e.g. a resvg `Pixmap`), skipping the premultiply step `new()` performs.
    pub fn from_premultiplied(
        pixels: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<Self, ImageDataError> {
        check_fills(&pixels, width, height)?;
        Ok(Self::addressed(pixels, width, height))
    }

    /// Refers to a texture the application owns and keeps filling, rather than pixels Telar uploads.
    ///
    /// `id` addresses the texture *object*, not its contents: the whole point is that the contents change every frame without Telar being told. Bump it only when the texture itself is replaced — a resize, a format change — so the bind group built against the old one is dropped.
    pub fn external(
        texture: std::sync::Arc<dyn ExternalTexture>,
        id: u64,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            id,
            source: ImageSource::External(texture),
            width,
            height,
        }
    }

    /// A picture at `url`, `width`×`height` pixels, with `variants` smaller copies of it in any order: see [`Linked`]. Addressed by its URL, which names the content: the build names each file by its hash.
    pub fn linked(url: &str, width: u32, height: u32, variants: &[(u32, &str)]) -> Self {
        let mut variants: Vec<(u32, std::sync::Arc<str>)> = variants
            .iter()
            .map(|(width, url)| (*width, std::sync::Arc::from(*url)))
            .collect();
        variants.sort_by_key(|(width, _)| *width);
        Self::from_linked(
            Linked {
                url: url.into(),
                variants: variants.into(),
            },
            width,
            height,
        )
    }

    pub(crate) fn from_linked(linked: Linked, width: u32, height: u32) -> Self {
        let seed = ((width as u64) << 32) | height as u64;
        Self {
            id: xxh3_64_with_seed(linked.url.as_bytes(), seed),
            source: ImageSource::Linked(std::sync::Arc::new(linked)),
            width,
            height,
        }
    }

    /// Content address: equal pixels at equal dimensions give equal ids, whoever built them and whenever.
    ///
    /// The renderers key their texture caches on this and nothing else, so the id has to identify the *image*. A per-construction counter identified the allocation instead: a caller that rebuilt the same image each frame — which is what building an `ImageData` inside a widget body does — minted a fresh key every time, and every entry behind it became unreachable weight the cache could only shed by hitting its byte budget.
    ///
    /// An external texture cannot be hashed, so its owner supplies the id and carries the same duty: keep it stable while the texture is, and change it when the texture object is replaced.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub(crate) fn without_pixels(id: u64, width: u32, height: u32) -> Self {
        Self {
            id,
            source: ImageSource::Pixels(Vec::new()),
            width,
            height,
        }
    }

    /// Where the picture lives, for one that is not in the binary.
    pub fn linked_source(&self) -> Option<&Linked> {
        match &self.source {
            ImageSource::Linked(linked) => Some(linked),
            _ => None,
        }
    }

    /// The premultiplied RGBA8 bytes; empty when the picture lives in a texture Telar does not own.
    ///
    /// Empty rather than `Option` so a backend that cannot use an external texture needs no special case: every path that turns these bytes into a raster already has to reject a buffer too short for the dimensions, and an empty one takes that branch.
    pub fn pixels(&self) -> &[u8] {
        match &self.source {
            ImageSource::Pixels(p) => p,
            ImageSource::External(_) | ImageSource::Linked(_) => &[],
        }
    }

    pub fn external_texture(&self) -> Option<&std::sync::Arc<dyn ExternalTexture>> {
        match &self.source {
            ImageSource::External(t) => Some(t),
            ImageSource::Pixels(_) | ImageSource::Linked(_) => None,
        }
    }

    // Hashed after premultiplication, so both constructors address the same finished image alike. The dimensions ride in as the seed rather than leading bytes: one buffer can be several images, and seeding keeps that distinction while leaving the pixels a single one-shot pass.
    fn addressed(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        let seed = ((width as u64) << 32) | height as u64;
        Self {
            id: xxh3_64_with_seed(&pixels, seed),
            source: ImageSource::Pixels(pixels),
            width,
            height,
        }
    }
}

fn check_fills(pixels: &[u8], width: u32, height: u32) -> Result<(), ImageDataError> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|area| area.checked_mul(4));
    match expected == Some(pixels.len()) {
        true => Ok(()),
        false => Err(ImageDataError {
            width,
            height,
            bytes: pixels.len(),
        }),
    }
}

#[inline]
/// Premultiplies straight-alpha RGBA bytes in place.
pub fn premultiply_rgba(pixels: &mut [u8]) {
    let (chunks, rest) = pixels.as_chunks_mut::<16>();
    for chunk in chunks {
        let r = u32x4::new([
            chunk[0] as u32,
            chunk[4] as u32,
            chunk[8] as u32,
            chunk[12] as u32,
        ]);
        let g = u32x4::new([
            chunk[1] as u32,
            chunk[5] as u32,
            chunk[9] as u32,
            chunk[13] as u32,
        ]);
        let b = u32x4::new([
            chunk[2] as u32,
            chunk[6] as u32,
            chunk[10] as u32,
            chunk[14] as u32,
        ]);
        let a = u32x4::new([
            chunk[3] as u32,
            chunk[7] as u32,
            chunk[11] as u32,
            chunk[15] as u32,
        ]);
        let bias = u32x4::splat(128);
        let shift = u32x4::splat(8);
        let scale = |channel: u32x4| {
            let product = channel * a + bias;
            (product + (product >> shift)) >> shift
        };
        let r_new = scale(r);
        let g_new = scale(g);
        let b_new = scale(b);
        let ra = r_new.to_array();
        let ga = g_new.to_array();
        let ba = b_new.to_array();
        chunk[0] = ra[0] as u8;
        chunk[4] = ra[1] as u8;
        chunk[8] = ra[2] as u8;
        chunk[12] = ra[3] as u8;
        chunk[1] = ga[0] as u8;
        chunk[5] = ga[1] as u8;
        chunk[9] = ga[2] as u8;
        chunk[13] = ga[3] as u8;
        chunk[2] = ba[0] as u8;
        chunk[6] = ba[1] as u8;
        chunk[10] = ba[2] as u8;
        chunk[14] = ba[3] as u8;
    }
    for chunk in rest.as_chunks_mut::<4>().0 {
        let a = chunk[3] as u32;
        for channel in &mut chunk[..3] {
            *channel = scale_by_alpha(*channel as u32, a);
        }
    }
}

fn scale_by_alpha(channel: u32, alpha: u32) -> u8 {
    let product = channel * alpha + 128;
    ((product + (product >> 8)) >> 8) as u8
}

#[cfg(test)]
#[path = "image.test.rs"]
mod tests;
