//! The text the catalogue draws itself, in the locales it ships, and the one way a widget asks for it.

use telar::i18n::{Catalog, Entry, Message, translate_with_override};

const NAMESPACE: &str = "telar_components";

pub(crate) const CLOSE: &str = "close";
pub(crate) const RESIZE: &str = "resize";
pub(crate) const SELECT: &str = "select";

static CATALOG: Catalog = Catalog {
    locales: &["ar", "en", "es"],
    default_locale: "en",
    // Sorted by key: a lookup is a binary search, and an entry out of order is never found.
    entries: &[
        Entry {
            key: CLOSE,
            messages: &[
                ("ar", Message::Plain("إغلاق")),
                ("en", Message::Plain("Close")),
                ("es", Message::Plain("Cerrar")),
            ],
        },
        Entry {
            key: RESIZE,
            messages: &[
                ("ar", Message::Plain("تغيير الحجم")),
                ("en", Message::Plain("Resize")),
                ("es", Message::Plain("Redimensionar")),
            ],
        },
        Entry {
            key: SELECT,
            messages: &[
                ("ar", Message::Plain("اختر")),
                ("en", Message::Plain("Select")),
                ("es", Message::Plain("Seleccionar")),
            ],
        },
    ],
};

/// Resolves `key` for the active locale, preferring the application's `telar_components.<key>` message over the one shipped here. Call it inside a reactive closure so a locale switch re-renders.
pub(crate) fn text(key: &str) -> String {
    translate_with_override(NAMESPACE, &CATALOG, key, &[])
}
