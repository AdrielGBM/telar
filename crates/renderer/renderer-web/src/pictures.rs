//! Linked pictures for a canvas: fetched and decoded by the browser, then drawn like any other bitmap.
//!
//! A picture that has not arrived is left out of the frame rather than drawn as a placeholder: layout already holds its box, and the frame after it lands draws it there.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use renderer_core::{DrawCommand, ImageData, Linked};
use wasm_bindgen::{JsCast, JsValue};

enum Arrival {
    Loading,
    Ready(Arc<ImageData>),
    Failed,
}

/// Every linked picture a frame has asked for, by address.
#[derive(Default)]
pub(crate) struct Pictures {
    arrivals: Rc<RefCell<HashMap<Arc<str>, Arrival>>>,
}

impl Pictures {
    /// `commands` with each linked picture swapped for its pixels, or left out while they are on their way. Borrowed as they are when nothing in them is linked, which is every frame of an app without pictures.
    pub(crate) fn resolve<'a>(&self, commands: &'a [DrawCommand]) -> Cow<'a, [DrawCommand]> {
        let linked = |command: &DrawCommand| matches!(command, DrawCommand::Image { data, .. } if data.linked_source().is_some());
        if !commands.iter().any(linked) {
            return Cow::Borrowed(commands);
        }
        let mut resolved = Vec::with_capacity(commands.len());
        for command in commands {
            let DrawCommand::Image {
                data,
                rect,
                raster,
                fill,
            } = command
            else {
                resolved.push(command.clone());
                continue;
            };
            let Some(source) = data.linked_source() else {
                resolved.push(command.clone());
                continue;
            };
            if let Some(pixels) = self.pixels_for(source, data.width, rect.width) {
                resolved.push(DrawCommand::Image {
                    data: pixels,
                    rect: *rect,
                    raster: *raster,
                    fill: *fill,
                });
            }
        }
        Cow::Owned(resolved)
    }

    /// The copy that covers `drawn_width` device pixels, asked for if it has not been; while it is on its way, any copy of the same picture that has arrived.
    fn pixels_for(&self, source: &Linked, width: u32, drawn_width: f32) -> Option<Arc<ImageData>> {
        let wanted = source
            .variants
            .iter()
            .find(|(copy_width, _)| *copy_width as f32 >= drawn_width)
            .map_or(&source.url, |(_, url)| url);
        let ready = |url: &Arc<str>| match self.arrivals.borrow().get(url) {
            Some(Arrival::Ready(pixels)) => Some(Arc::clone(pixels)),
            _ => None,
        };
        if let Some(pixels) = ready(wanted) {
            return Some(pixels);
        }
        if !self.arrivals.borrow().contains_key(wanted) {
            self.fetch(Arc::clone(wanted));
        }
        std::iter::once((width, &source.url))
            .chain(source.variants.iter().rev().map(|(w, url)| (*w, url)))
            .find_map(|(_, url)| ready(url))
    }

    fn fetch(&self, url: Arc<str>) {
        self.arrivals
            .borrow_mut()
            .insert(Arc::clone(&url), Arrival::Loading);
        let arrivals = Rc::clone(&self.arrivals);
        wasm_bindgen_futures::spawn_local(async move {
            let address = platform_core::asset_url(&url);
            let arrival = match decode(&address).await {
                Ok(pixels) => Arrival::Ready(Arc::new(pixels)),
                Err(error) => {
                    tracing::warn!(%address, ?error, "a picture could not be loaded");
                    Arrival::Failed
                }
            };
            arrivals.borrow_mut().insert(url, arrival);
            if let Some(wake) = platform_core::loop_waker() {
                wake();
            }
        });
    }
}

/// The picture at `address` as pixels, decoded by the browser and read back through a canvas of its own size.
async fn decode(address: &str) -> Result<ImageData, JsValue> {
    let image = web_sys::HtmlImageElement::new()?;
    image.set_src(address);
    wasm_bindgen_futures::JsFuture::from(image.decode()).await?;
    let (width, height) = (image.natural_width(), image.natural_height());
    let document = crate::dom_window()
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let canvas: web_sys::HtmlCanvasElement = document.create_element("canvas")?.dyn_into()?;
    canvas.set_width(width);
    canvas.set_height(height);
    let context: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("no 2d context"))?
        .dyn_into()?;
    context.draw_image_with_html_image_element(&image, 0.0, 0.0)?;
    let pixels = context.get_image_data(0.0, 0.0, width as f64, height as f64)?;
    Ok(ImageData::new(pixels.data().0, width, height))
}

#[cfg(test)]
#[path = "pictures_test.rs"]
mod tests;
