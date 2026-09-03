//! A bitmap as something a document can load.
//!
//! Encoded through a detached canvas rather than a PNG encoder of Telar's own: the browser already has one,
//! it runs at native speed, and the result is a string the document caches by itself. Encoding is the
//! expensive part, so it happens once per distinct picture — which [`ImageData`] makes easy by addressing
//! itself by content, so a widget that rebuilds the same bitmap every frame asks for the same entry.

use std::cell::RefCell;
use std::rc::Rc;

use renderer_core::ImageData;
use rustc_hash::FxHashMap;
use wasm_bindgen::{Clamped, JsCast};

/// Past this many distinct pictures the cache is holding artwork nothing is drawing any more, which only a
/// caller that mints a new image every frame can reach. Dropped whole: the entries are re-encoded on demand,
/// and any policy finer than that would be guessing at which of them is still on the surface.
const LIMIT: usize = 64;

thread_local! {
    static CACHE: RefCell<FxHashMap<u64, Rc<str>>> = RefCell::new(FxHashMap::default());
}

/// The `href` this picture is drawn from, or `None` for one this backend cannot read — a texture the
/// application owns and fills on the GPU has no pixels on this side to encode.
pub fn href(data: &ImageData) -> Option<Rc<str>> {
    if let Some(cached) = CACHE.with(|cache| cache.borrow().get(&data.id).cloned()) {
        return Some(cached);
    }
    let url = encode(data)?;
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= LIMIT {
            cache.clear();
        }
        cache.insert(data.id, url.clone());
    });
    Some(url)
}

fn encode(data: &ImageData) -> Option<Rc<str>> {
    let (width, height) = (data.width, data.height);
    let pixels = data.pixels();
    if width == 0 || height == 0 || pixels.len() < width as usize * height as usize * 4 {
        return None;
    }
    // Telar keeps its pixels premultiplied and a canvas takes them straight, so the alpha has to come back
    // out before they are handed over — otherwise every partly transparent picture arrives darkened.
    let mut straight = pixels.to_vec();
    unpremultiply(&mut straight);

    let document = web_sys::window()?.document()?;
    let canvas = document
        .create_element("canvas")
        .ok()?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .ok()?;
    canvas.set_width(width);
    canvas.set_height(height);
    let context = canvas
        .get_context("2d")
        .ok()??
        .dyn_into::<web_sys::CanvasRenderingContext2d>()
        .ok()?;
    let image =
        web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&straight), width, height)
            .ok()?;
    context.put_image_data(&image, 0.0, 0.0).ok()?;
    canvas.to_data_url_with_type("image/png").ok().map(Rc::from)
}

fn unpremultiply(pixels: &mut [u8]) {
    for chunk in pixels.chunks_exact_mut(4) {
        let alpha = chunk[3];
        if alpha == 0 || alpha == 255 {
            continue;
        }
        let alpha = alpha as u32;
        for channel in &mut chunk[..3] {
            *channel = ((*channel as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
}
