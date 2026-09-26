//! The faces a page ships, fetched into the shaper a canvas draws with.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// What a font preload carries to name the family its face answers to. The web packaging writes it on every face `[[telar.fonts]]` declares.
pub const FAMILY_ATTRIBUTE: &str = "data-telar-family";

/// Fetches every face the page preloads for this app and adds each to the shaper's database as it lands.
///
/// A canvas cannot use an `@font-face`: its glyphs are shaped from bytes, and the browser never hands a loaded face's bytes to the page. So the faces are fetched again — from the cache the preload already filled, since both are anonymous CORS requests for the same URL. Nothing waits for them: the app starts drawing in the faces it has, and each one that lands re-measures and redraws once.
pub fn load_page_fonts() {
    let Some(document) = crate::dom_window().document() else {
        return;
    };
    let Ok(links) =
        document.query_selector_all(&format!("link[rel~=preload][as=font][{FAMILY_ATTRIBUTE}]"))
    else {
        return;
    };
    for index in 0..links.length() {
        let Some(link) = links
            .item(index)
            .and_then(|node| node.dyn_into::<web_sys::HtmlLinkElement>().ok())
        else {
            continue;
        };
        let family = link.get_attribute(FAMILY_ATTRIBUTE).unwrap_or_default();
        let href = link.href();
        wasm_bindgen_futures::spawn_local(async move {
            match fetch_bytes(&href).await {
                Ok(bytes) if is_sfnt(&bytes) => {
                    let face = renderer_core::FontAsset::bytes(bytes);
                    let face = match family.is_empty() {
                        true => face,
                        false => face.named(family),
                    };
                    renderer_text::fonts::add_faces(vec![face]);
                    if let Some(wake) = platform_core::loop_waker() {
                        wake();
                    }
                }
                Ok(_) => tracing::debug!(
                    "{href} is a WOFF file, which only a document can use; a canvas needs the face as TTF or OTF"
                ),
                Err(e) => tracing::warn!("could not fetch the font {href}: {e:?}"),
            }
        });
    }
}

async fn fetch_bytes(url: &str) -> Result<Vec<u8>, JsValue> {
    let response: web_sys::Response = JsFuture::from(crate::dom_window().fetch_with_str(url))
        .await?
        .dyn_into()?;
    if !response.ok() {
        return Err(JsValue::from_str(&format!("HTTP {}", response.status())));
    }
    let buffer = JsFuture::from(response.array_buffer()?).await?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}

/// Whether the bytes are a TrueType or OpenType face, which a shaper reads, rather than a WOFF wrapper, which only a browser unpacks.
fn is_sfnt(bytes: &[u8]) -> bool {
    matches!(
        bytes.get(..4),
        Some([0, 1, 0, 0] | b"OTTO" | b"true" | b"ttcf")
    )
}
