//! Parsing a runtime-chosen SVG once instead of once per render.
//!
//! `svg src:"literal"` is baked at build time, but an icon picked at runtime — a tool table, a feature kind, a themed glyph set — cannot be a literal, and `SvgData::from_str` on every render is a full parse per frame. Every application that needed this wrote the same thread-local memo; this is that memo, once.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use crate::svg::SvgData;

thread_local! {
    static PARSED: RefCell<HashMap<u64, Option<Arc<SvgData>>>> = RefCell::new(HashMap::new());
}

/// The parsed form of `key`'s SVG, parsing it on the first ask and handing back the same `Arc` after that. `None` for a source that does not parse — remembered too, so a broken glyph is not re-parsed every frame.
///
/// `key` identifies the *source*, not the call site. Two natural spellings: the pointer of a `&'static [u8]` or `&'static str` that a baked table hands back (stable for the life of the process, and free), or a hash of the icon's name. Anything that collides will hand back the wrong glyph, so it must identify the source exactly.
///
/// Thread-local because `SvgData` is what a widget draws with, and widgets live on one thread.
pub fn svg_cached(key: u64, source: impl FnOnce() -> Option<String>) -> Option<Arc<SvgData>> {
    PARSED.with(|cache| {
        if let Some(found) = cache.borrow().get(&key) {
            return found.clone();
        }
        let parsed = source()
            .and_then(|text| SvgData::from_str(&text).ok())
            .map(Arc::new);
        cache.borrow_mut().insert(key, parsed.clone());
        parsed
    })
}

/// The cache key for a `&'static` source a baked table owns, which is what an icon registry hands back.
pub fn static_key<T: ?Sized>(source: &'static T) -> u64 {
    std::ptr::from_ref(source).cast::<u8>() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const CIRCLE: &str = r#"<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="4"/></svg>"#;

    #[test]
    fn a_source_is_parsed_once_and_shared_after_that() {
        let mut parses = 0;
        let key = static_key(CIRCLE);
        let first = svg_cached(key, || {
            parses += 1;
            Some(CIRCLE.to_string())
        });
        let second = svg_cached(key, || {
            parses += 1;
            Some(CIRCLE.to_string())
        });
        assert_eq!(parses, 1, "the second ask never reached the parser");
        assert!(first.is_some());
        assert!(
            Arc::ptr_eq(&first.unwrap(), &second.unwrap()),
            "both asks share one parse"
        );
    }

    /// A glyph that does not parse is remembered as such: re-parsing broken input every frame is the failure this cache exists to avoid, and it is the case that would keep hitting the parser.
    #[test]
    fn a_source_that_does_not_parse_is_remembered_too() {
        let mut parses = 0;
        let key = static_key("not an svg at all");
        for _ in 0..2 {
            assert!(
                svg_cached(key, || {
                    parses += 1;
                    Some("not an svg at all".to_string())
                })
                .is_none()
            );
        }
        assert_eq!(parses, 1);
    }
}
