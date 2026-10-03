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
        slot: crate::document::picture_slot(width),
    };
    if shown.as_ref() == Some(&now) {
        return;
    }
    for (name, value) in crate::document::picture_attributes(picture, width) {
        crate::reconcile::set_or_clear(node, name, value.as_deref());
    }
    *shown = Some(now);
}
