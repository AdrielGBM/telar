//! Loading a typeface that arrives at run time.

use std::sync::Arc;

use ui_core::{AssetDecoder, AssetError};

/// Decodes a font file and makes its faces available to every shaper in the process, handing back the family name to ask for them by.
///
/// The gap this closes: until now the only way into the font database was [`AppConfig::font_data`](https://docs.rs/telar), which is bytes handed over before the first frame. A face that is downloaded, picked by the user, or shipped in a language pack had nowhere to go — and a caller could not name it in a [`TextStyle`](https://docs.rs/telar) anyway, because the family a file turns out to declare is not something the caller knows.
///
/// So the decoded value *is* the family name:
///
/// ```ignore
/// let faces = AssetLoader::new(transport, Some(cache), Arc::new(FontDecoder));
/// // In a view: the text renders in the platform's font until the face lands, then in the face.
/// match faces.get("Inter").get() {
///     AssetState::Ready(family) => style.with_font_family(family.to_string()),
///     _ => style,
/// }
/// ```
///
/// Loading is additive, like every other way into the database: a shaper already built keeps every face it was built from, so a face arriving late cannot pull one out from under a surface that is drawing.
///
/// A file carrying several faces reports the first family it declares. The rest are loaded and reachable by their own names — this returns one because a caller asking for "the family this file is" wants a name to put in a style, not a set to choose from.
pub struct FontDecoder;

impl AssetDecoder for FontDecoder {
    fn kind(&self) -> &'static str {
        "font"
    }

    type Output = Arc<str>;

    fn decode(&self, bytes: &[u8]) -> Result<Self::Output, AssetError> {
        let families = renderer_text::fonts::install_face(bytes.to_vec())
            .ok_or_else(|| AssetError("not a font file this build can read".to_string()))?;
        families
            .into_iter()
            .next()
            .map(Arc::from)
            .ok_or_else(|| AssetError("font declares no family name".to_string()))
    }
}

#[cfg(test)]
#[path = "font_test.rs"]
mod tests;
