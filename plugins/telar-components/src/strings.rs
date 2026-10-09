//! The text the catalogue draws itself, in the locales it ships, and the one way a widget asks for it.

use telar::i18n::{Catalog, Entry, Message, translate_with_override};

const NAMESPACE: &str = "telar_components";

pub(crate) const CLOSE: &str = "close";
pub(crate) const COMMAND_PALETTE: &str = "command_palette";
pub(crate) const COPIED: &str = "copied";
pub(crate) const COPY: &str = "copy";
pub(crate) const DISMISS: &str = "dismiss";
pub(crate) const HEX: &str = "hex";
pub(crate) const HUE: &str = "hue";
pub(crate) const NO_RESULTS: &str = "no_results";
pub(crate) const NOTIFICATIONS: &str = "notifications";
pub(crate) const OPACITY: &str = "opacity";
pub(crate) const RESIZE: &str = "resize";
pub(crate) const SATURATION_BRIGHTNESS: &str = "saturation_brightness";
pub(crate) const SEARCH_COMMANDS: &str = "search_commands";
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
            key: COMMAND_PALETTE,
            messages: &[
                ("ar", Message::Plain("لوحة الأوامر")),
                ("en", Message::Plain("Command palette")),
                ("es", Message::Plain("Paleta de comandos")),
            ],
        },
        Entry {
            key: COPIED,
            messages: &[
                ("ar", Message::Plain("تم النسخ")),
                ("en", Message::Plain("Copied")),
                ("es", Message::Plain("Copiado")),
            ],
        },
        Entry {
            key: COPY,
            messages: &[
                ("ar", Message::Plain("نسخ")),
                ("en", Message::Plain("Copy")),
                ("es", Message::Plain("Copiar")),
            ],
        },
        Entry {
            key: DISMISS,
            messages: &[
                ("ar", Message::Plain("تجاهل")),
                ("en", Message::Plain("Dismiss")),
                ("es", Message::Plain("Descartar")),
            ],
        },
        Entry {
            key: HEX,
            messages: &[
                ("ar", Message::Plain("الرمز الست عشري")),
                ("en", Message::Plain("Hex")),
                ("es", Message::Plain("Hexadecimal")),
            ],
        },
        Entry {
            key: HUE,
            messages: &[
                ("ar", Message::Plain("درجة اللون")),
                ("en", Message::Plain("Hue")),
                ("es", Message::Plain("Tono")),
            ],
        },
        Entry {
            key: NO_RESULTS,
            messages: &[
                ("ar", Message::Plain("لا توجد نتائج")),
                ("en", Message::Plain("No results")),
                ("es", Message::Plain("Sin resultados")),
            ],
        },
        Entry {
            key: NOTIFICATIONS,
            messages: &[
                ("ar", Message::Plain("الإشعارات")),
                ("en", Message::Plain("Notifications")),
                ("es", Message::Plain("Notificaciones")),
            ],
        },
        Entry {
            key: OPACITY,
            messages: &[
                ("ar", Message::Plain("العتامة")),
                ("en", Message::Plain("Opacity")),
                ("es", Message::Plain("Opacidad")),
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
            key: SATURATION_BRIGHTNESS,
            messages: &[
                ("ar", Message::Plain("التشبع والسطوع")),
                ("en", Message::Plain("Saturation and brightness")),
                ("es", Message::Plain("Saturación y brillo")),
            ],
        },
        Entry {
            key: SEARCH_COMMANDS,
            messages: &[
                ("ar", Message::Plain("اكتب أمرًا أو ابحث…")),
                ("en", Message::Plain("Type a command or search…")),
                ("es", Message::Plain("Escribe un comando o busca…")),
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

#[cfg(test)]
#[path = "strings_catalog_test.rs"]
mod tests;
