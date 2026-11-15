//! The text the catalogue draws itself, in the locales it ships, and the one way a widget asks for it.

use telar::i18n::{Catalog, Entry, Message, Part, translate_with_override};

const NAMESPACE: &str = "telar_components";

pub(crate) const BRIGHTNESS: &str = "brightness";
pub(crate) const CHARACTERS: &str = "characters";
pub(crate) const CHARACTERS_OF: &str = "characters_of";
pub(crate) const CLEAR_SEARCH: &str = "clear_search";
pub(crate) const CLOSE: &str = "close";
pub(crate) const CODE: &str = "code";
pub(crate) const COMMAND_PALETTE: &str = "command_palette";
pub(crate) const COMMANDS: &str = "commands";
pub(crate) const COPIED: &str = "copied";
pub(crate) const COPY: &str = "copy";
pub(crate) const DISMISS: &str = "dismiss";
pub(crate) const DRAWER: &str = "drawer";
pub(crate) const ERROR: &str = "error";
pub(crate) const EXTERNAL_LINK: &str = "external_link";
pub(crate) const HEX: &str = "hex";
pub(crate) const HUE: &str = "hue";
pub(crate) const INFORMATION: &str = "information";
pub(crate) const MAXIMIZE: &str = "maximize";
pub(crate) const MINIMIZE: &str = "minimize";
pub(crate) const NO_RESULTS: &str = "no_results";
pub(crate) const NOTHING_TO_SHOW: &str = "nothing_to_show";
pub(crate) const NOTICE: &str = "notice";
pub(crate) const NOTIFICATIONS: &str = "notifications";
pub(crate) const OPACITY: &str = "opacity";
pub(crate) const RESIZE: &str = "resize";
pub(crate) const RESIZE_COLUMN: &str = "resize_column";
pub(crate) const RESIZE_HEIGHT: &str = "resize_height";
pub(crate) const RESIZE_WIDTH: &str = "resize_width";
pub(crate) const RESTORE: &str = "restore";
pub(crate) const SATURATION: &str = "saturation";
pub(crate) const SATURATION_BRIGHTNESS: &str = "saturation_brightness";
pub(crate) const SEARCH: &str = "search";
pub(crate) const SEARCH_COMMANDS: &str = "search_commands";
pub(crate) const SELECT: &str = "select";
pub(crate) const SUCCESS: &str = "success";
pub(crate) const THEN: &str = "then";
pub(crate) const TITLED_NOTICE: &str = "titled_notice";
pub(crate) const WARNING: &str = "warning";
pub(crate) const WINDOW: &str = "window";

static CATALOG: Catalog = Catalog {
    locales: &["ar", "en", "es"],
    default_locale: "en",
    // Sorted by key: a lookup is a binary search, and an entry out of order is never found.
    entries: &[
        Entry {
            key: BRIGHTNESS,
            messages: &[
                ("ar", Message::Plain("السطوع")),
                ("en", Message::Plain("Brightness")),
                ("es", Message::Plain("Brillo")),
            ],
        },
        Entry {
            key: CHARACTERS,
            messages: &[
                (
                    "ar",
                    Message::Format(&[Part::Arg("count"), Part::Lit(" حرف")]),
                ),
                (
                    "en",
                    Message::Format(&[Part::Arg("count"), Part::Lit(" characters")]),
                ),
                (
                    "es",
                    Message::Format(&[Part::Arg("count"), Part::Lit(" caracteres")]),
                ),
            ],
        },
        Entry {
            key: CHARACTERS_OF,
            messages: &[
                (
                    "ar",
                    Message::Format(&[
                        Part::Arg("count"),
                        Part::Lit(" من "),
                        Part::Arg("max"),
                        Part::Lit(" حرف"),
                    ]),
                ),
                (
                    "en",
                    Message::Format(&[
                        Part::Arg("count"),
                        Part::Lit(" of "),
                        Part::Arg("max"),
                        Part::Lit(" characters"),
                    ]),
                ),
                (
                    "es",
                    Message::Format(&[
                        Part::Arg("count"),
                        Part::Lit(" de "),
                        Part::Arg("max"),
                        Part::Lit(" caracteres"),
                    ]),
                ),
            ],
        },
        Entry {
            key: CLEAR_SEARCH,
            messages: &[
                ("ar", Message::Plain("مسح البحث")),
                ("en", Message::Plain("Clear search")),
                ("es", Message::Plain("Borrar búsqueda")),
            ],
        },
        Entry {
            key: CLOSE,
            messages: &[
                ("ar", Message::Plain("إغلاق")),
                ("en", Message::Plain("Close")),
                ("es", Message::Plain("Cerrar")),
            ],
        },
        Entry {
            key: CODE,
            messages: &[
                ("ar", Message::Plain("الكود")),
                ("en", Message::Plain("Code")),
                ("es", Message::Plain("Código")),
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
            key: COMMANDS,
            messages: &[
                ("ar", Message::Plain("الأوامر")),
                ("en", Message::Plain("Commands")),
                ("es", Message::Plain("Comandos")),
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
            key: DRAWER,
            messages: &[
                ("ar", Message::Plain("درج جانبي")),
                ("en", Message::Plain("Drawer")),
                ("es", Message::Plain("Panel lateral")),
            ],
        },
        Entry {
            key: ERROR,
            messages: &[
                ("ar", Message::Plain("خطأ")),
                ("en", Message::Plain("Error")),
                ("es", Message::Plain("Error")),
            ],
        },
        Entry {
            key: EXTERNAL_LINK,
            messages: &[
                (
                    "ar",
                    Message::Format(&[Part::Arg("label"), Part::Lit(" (رابط خارجي)")]),
                ),
                (
                    "en",
                    Message::Format(&[Part::Arg("label"), Part::Lit(" (external link)")]),
                ),
                (
                    "es",
                    Message::Format(&[Part::Arg("label"), Part::Lit(" (enlace externo)")]),
                ),
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
            key: INFORMATION,
            messages: &[
                ("ar", Message::Plain("معلومة")),
                ("en", Message::Plain("Information")),
                ("es", Message::Plain("Información")),
            ],
        },
        Entry {
            key: MAXIMIZE,
            messages: &[
                ("ar", Message::Plain("تكبير")),
                ("en", Message::Plain("Maximize")),
                ("es", Message::Plain("Maximizar")),
            ],
        },
        Entry {
            key: MINIMIZE,
            messages: &[
                ("ar", Message::Plain("تصغير")),
                ("en", Message::Plain("Minimize")),
                ("es", Message::Plain("Minimizar")),
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
            key: NOTHING_TO_SHOW,
            messages: &[
                ("ar", Message::Plain("لا يوجد شيء لعرضه")),
                ("en", Message::Plain("Nothing to show")),
                ("es", Message::Plain("No hay nada que mostrar")),
            ],
        },
        Entry {
            key: NOTICE,
            messages: &[
                (
                    "ar",
                    Message::Format(&[Part::Arg("tone"), Part::Lit(": "), Part::Arg("message")]),
                ),
                (
                    "en",
                    Message::Format(&[Part::Arg("tone"), Part::Lit(": "), Part::Arg("message")]),
                ),
                (
                    "es",
                    Message::Format(&[Part::Arg("tone"), Part::Lit(": "), Part::Arg("message")]),
                ),
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
            key: RESIZE_COLUMN,
            messages: &[
                (
                    "ar",
                    Message::Format(&[Part::Lit("تغيير عرض العمود "), Part::Arg("label")]),
                ),
                (
                    "en",
                    Message::Format(&[
                        Part::Lit("Resize "),
                        Part::Arg("label"),
                        Part::Lit(" column"),
                    ]),
                ),
                (
                    "es",
                    Message::Format(&[
                        Part::Lit("Cambiar el ancho de la columna "),
                        Part::Arg("label"),
                    ]),
                ),
            ],
        },
        Entry {
            key: RESIZE_HEIGHT,
            messages: &[
                ("ar", Message::Plain("تغيير الارتفاع")),
                ("en", Message::Plain("Resize height")),
                ("es", Message::Plain("Cambiar el alto")),
            ],
        },
        Entry {
            key: RESIZE_WIDTH,
            messages: &[
                ("ar", Message::Plain("تغيير العرض")),
                ("en", Message::Plain("Resize width")),
                ("es", Message::Plain("Cambiar el ancho")),
            ],
        },
        Entry {
            key: RESTORE,
            messages: &[
                ("ar", Message::Plain("استعادة")),
                ("en", Message::Plain("Restore")),
                ("es", Message::Plain("Restaurar")),
            ],
        },
        Entry {
            key: SATURATION,
            messages: &[
                ("ar", Message::Plain("التشبع")),
                ("en", Message::Plain("Saturation")),
                ("es", Message::Plain("Saturación")),
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
            key: SEARCH,
            messages: &[
                ("ar", Message::Plain("بحث")),
                ("en", Message::Plain("Search")),
                ("es", Message::Plain("Buscar")),
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
        Entry {
            key: SUCCESS,
            messages: &[
                ("ar", Message::Plain("نجاح")),
                ("en", Message::Plain("Success")),
                ("es", Message::Plain("Éxito")),
            ],
        },
        Entry {
            key: THEN,
            messages: &[
                (
                    "ar",
                    Message::Format(&[Part::Arg("first"), Part::Lit("، ثم "), Part::Arg("next")]),
                ),
                (
                    "en",
                    Message::Format(&[Part::Arg("first"), Part::Lit(", then "), Part::Arg("next")]),
                ),
                (
                    "es",
                    Message::Format(&[
                        Part::Arg("first"),
                        Part::Lit(", luego "),
                        Part::Arg("next"),
                    ]),
                ),
            ],
        },
        Entry {
            key: TITLED_NOTICE,
            messages: &[
                (
                    "ar",
                    Message::Format(&[
                        Part::Arg("tone"),
                        Part::Lit(": "),
                        Part::Arg("title"),
                        Part::Lit(". "),
                        Part::Arg("message"),
                    ]),
                ),
                (
                    "en",
                    Message::Format(&[
                        Part::Arg("tone"),
                        Part::Lit(": "),
                        Part::Arg("title"),
                        Part::Lit(". "),
                        Part::Arg("message"),
                    ]),
                ),
                (
                    "es",
                    Message::Format(&[
                        Part::Arg("tone"),
                        Part::Lit(": "),
                        Part::Arg("title"),
                        Part::Lit(". "),
                        Part::Arg("message"),
                    ]),
                ),
            ],
        },
        Entry {
            key: WARNING,
            messages: &[
                ("ar", Message::Plain("تحذير")),
                ("en", Message::Plain("Warning")),
                ("es", Message::Plain("Advertencia")),
            ],
        },
        Entry {
            key: WINDOW,
            messages: &[
                ("ar", Message::Plain("النافذة")),
                ("en", Message::Plain("Window")),
                ("es", Message::Plain("Ventana")),
            ],
        },
    ],
};

/// Resolves `key` for the active locale, preferring the application's `telar_components.<key>` message over the one shipped here. Call it inside a reactive closure so a locale switch re-renders.
pub(crate) fn text(key: &str) -> String {
    translate_with_override(NAMESPACE, &CATALOG, key, &[])
}

/// [`text`] for a message with named placeholders, filled from `args`.
pub(crate) fn text_with(key: &str, args: &[(&str, &str)]) -> String {
    translate_with_override(NAMESPACE, &CATALOG, key, args)
}

#[cfg(test)]
#[path = "strings.catalog.test.rs"]
mod tests;
