//! A linked picture as an `<img>`: the browser fetches it, decodes it off the main thread and picks the copy that fits the box.

use std::sync::Arc;

use renderer_core::Picture;

/// What was last written to an `<img>`, so an unchanged frame writes nothing.
#[derive(PartialEq)]
pub struct Shown {
    picture: Arc<Picture>,
    slot: u32,
}

/// Writes `picture` into `node`, laid out `width` CSS pixels wide, unless it already says so.
pub fn show(
    node: &web_sys::Element,
    picture: &Arc<Picture>,
    width: f32,
    shown: &mut Option<Shown>,
) {
    let now = Shown {
        picture: Arc::clone(picture),
        slot: width.max(1.0).ceil() as u32,
    };
    if shown.as_ref() == Some(&now) {
        return;
    }
    let source = &picture.source;
    let set = |name: &str, value: &str| {
        let _ = node.set_attribute(name, value);
    };
    // Before `src`: a browser that sees the address first starts fetching the full-size copy before it has been told there are smaller ones.
    if source.variants.is_empty() {
        let _ = node.remove_attribute("srcset");
        let _ = node.remove_attribute("sizes");
    } else {
        set("srcset", &srcset(picture));
        set("sizes", &format!("{}px", now.slot));
    }
    set("width", &picture.width.to_string());
    set("height", &picture.height.to_string());
    set("decoding", "async");
    if picture.priority {
        let _ = node.remove_attribute("loading");
        set("fetchpriority", "high");
    } else {
        set("loading", "lazy");
        let _ = node.remove_attribute("fetchpriority");
    }
    set("src", &platform_core::asset_url(&source.url));
    *shown = Some(now);
}

/// Every copy with the width it was made at, the full-size one last, which is what lets the browser weigh them against the box and the screen's density.
pub fn srcset(picture: &Picture) -> String {
    picture
        .source
        .variants
        .iter()
        .map(|(width, url)| (*width, url.as_ref()))
        .chain(std::iter::once((
            picture.width,
            picture.source.url.as_ref(),
        )))
        .map(|(width, url)| format!("{} {width}w", platform_core::asset_url(url)))
        .collect::<Vec<_>>()
        .join(", ")
}
