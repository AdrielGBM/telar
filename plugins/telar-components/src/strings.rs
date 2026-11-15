//! The text the catalogue draws itself, in the locales it ships, and the one way a widget asks for it.

use telar::i18n::{Catalog, Entry, Message, Part, locale_tag, translate_with_override};

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
pub(crate) const SWATCHES: &str = "swatches";
pub(crate) const THEN: &str = "then";
pub(crate) const TITLED_NOTICE: &str = "titled_notice";
pub(crate) const WARNING: &str = "warning";
pub(crate) const WINDOW: &str = "window";

static CATALOG: Catalog = Catalog {
    locales: &[locale_tag!("ar"), locale_tag!("en"), locale_tag!("es")],
    default_locale: locale_tag!("en"),
    // Sorted by key: a lookup is a binary search, and an entry out of order is never found.
    entries: &[
        Entry {
            key: BRIGHTNESS,
            messages: &[
                (locale_tag!("ar"), Message::Plain("السطوع")),
                (locale_tag!("en"), Message::Plain("Brightness")),
                (locale_tag!("es"), Message::Plain("Brillo")),
            ],
        },
        Entry {
            key: CHARACTERS,
            messages: &[
                (
                    locale_tag!("ar"),
                    Message::Format(&[Part::Arg("count"), Part::Lit(" حرف")]),
                ),
                (
                    locale_tag!("en"),
                    Message::Format(&[Part::Arg("count"), Part::Lit(" characters")]),
                ),
                (
                    locale_tag!("es"),
                    Message::Format(&[Part::Arg("count"), Part::Lit(" caracteres")]),
                ),
            ],
        },
        Entry {
            key: CHARACTERS_OF,
            messages: &[
                (
                    locale_tag!("ar"),
                    Message::Format(&[
                        Part::Arg("count"),
                        Part::Lit(" من "),
                        Part::Arg("max"),
                        Part::Lit(" حرف"),
                    ]),
                ),
                (
                    locale_tag!("en"),
                    Message::Format(&[
                        Part::Arg("count"),
                        Part::Lit(" of "),
                        Part::Arg("max"),
                        Part::Lit(" characters"),
                    ]),
                ),
                (
                    locale_tag!("es"),
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
                (locale_tag!("ar"), Message::Plain("مسح البحث")),
                (locale_tag!("en"), Message::Plain("Clear search")),
                (locale_tag!("es"), Message::Plain("Borrar búsqueda")),
            ],
        },
        Entry {
            key: CLOSE,
            messages: &[
                (locale_tag!("ar"), Message::Plain("إغلاق")),
                (locale_tag!("en"), Message::Plain("Close")),
                (locale_tag!("es"), Message::Plain("Cerrar")),
            ],
        },
        Entry {
            key: CODE,
            messages: &[
                (locale_tag!("ar"), Message::Plain("الكود")),
                (locale_tag!("en"), Message::Plain("Code")),
                (locale_tag!("es"), Message::Plain("Código")),
            ],
        },
        Entry {
            key: COMMAND_PALETTE,
            messages: &[
                (locale_tag!("ar"), Message::Plain("لوحة الأوامر")),
                (locale_tag!("en"), Message::Plain("Command palette")),
                (locale_tag!("es"), Message::Plain("Paleta de comandos")),
            ],
        },
        Entry {
            key: COMMANDS,
            messages: &[
                (locale_tag!("ar"), Message::Plain("الأوامر")),
                (locale_tag!("en"), Message::Plain("Commands")),
                (locale_tag!("es"), Message::Plain("Comandos")),
            ],
        },
        Entry {
            key: COPIED,
            messages: &[
                (locale_tag!("ar"), Message::Plain("تم النسخ")),
                (locale_tag!("en"), Message::Plain("Copied")),
                (locale_tag!("es"), Message::Plain("Copiado")),
            ],
        },
        Entry {
            key: COPY,
            messages: &[
                (locale_tag!("ar"), Message::Plain("نسخ")),
                (locale_tag!("en"), Message::Plain("Copy")),
                (locale_tag!("es"), Message::Plain("Copiar")),
            ],
        },
        Entry {
            key: DISMISS,
            messages: &[
                (locale_tag!("ar"), Message::Plain("تجاهل")),
                (locale_tag!("en"), Message::Plain("Dismiss")),
                (locale_tag!("es"), Message::Plain("Descartar")),
            ],
        },
        Entry {
            key: DRAWER,
            messages: &[
                (locale_tag!("ar"), Message::Plain("درج جانبي")),
                (locale_tag!("en"), Message::Plain("Drawer")),
                (locale_tag!("es"), Message::Plain("Panel lateral")),
            ],
        },
        Entry {
            key: ERROR,
            messages: &[
                (locale_tag!("ar"), Message::Plain("خطأ")),
                (locale_tag!("en"), Message::Plain("Error")),
                (locale_tag!("es"), Message::Plain("Error")),
            ],
        },
        Entry {
            key: EXTERNAL_LINK,
            messages: &[
                (
                    locale_tag!("ar"),
                    Message::Format(&[Part::Arg("label"), Part::Lit(" (رابط خارجي)")]),
                ),
                (
                    locale_tag!("en"),
                    Message::Format(&[Part::Arg("label"), Part::Lit(" (external link)")]),
                ),
                (
                    locale_tag!("es"),
                    Message::Format(&[Part::Arg("label"), Part::Lit(" (enlace externo)")]),
                ),
            ],
        },
        Entry {
            key: HEX,
            messages: &[
                (locale_tag!("ar"), Message::Plain("الرمز الست عشري")),
                (locale_tag!("en"), Message::Plain("Hex")),
                (locale_tag!("es"), Message::Plain("Hexadecimal")),
            ],
        },
        Entry {
            key: HUE,
            messages: &[
                (locale_tag!("ar"), Message::Plain("درجة اللون")),
                (locale_tag!("en"), Message::Plain("Hue")),
                (locale_tag!("es"), Message::Plain("Tono")),
            ],
        },
        Entry {
            key: INFORMATION,
            messages: &[
                (locale_tag!("ar"), Message::Plain("معلومة")),
                (locale_tag!("en"), Message::Plain("Information")),
                (locale_tag!("es"), Message::Plain("Información")),
            ],
        },
        Entry {
            key: MAXIMIZE,
            messages: &[
                (locale_tag!("ar"), Message::Plain("تكبير")),
                (locale_tag!("en"), Message::Plain("Maximize")),
                (locale_tag!("es"), Message::Plain("Maximizar")),
            ],
        },
        Entry {
            key: MINIMIZE,
            messages: &[
                (locale_tag!("ar"), Message::Plain("تصغير")),
                (locale_tag!("en"), Message::Plain("Minimize")),
                (locale_tag!("es"), Message::Plain("Minimizar")),
            ],
        },
        Entry {
            key: NO_RESULTS,
            messages: &[
                (locale_tag!("ar"), Message::Plain("لا توجد نتائج")),
                (locale_tag!("en"), Message::Plain("No results")),
                (locale_tag!("es"), Message::Plain("Sin resultados")),
            ],
        },
        Entry {
            key: NOTHING_TO_SHOW,
            messages: &[
                (locale_tag!("ar"), Message::Plain("لا يوجد شيء لعرضه")),
                (locale_tag!("en"), Message::Plain("Nothing to show")),
                (locale_tag!("es"), Message::Plain("No hay nada que mostrar")),
            ],
        },
        Entry {
            key: NOTICE,
            messages: &[
                (
                    locale_tag!("ar"),
                    Message::Format(&[Part::Arg("tone"), Part::Lit(": "), Part::Arg("message")]),
                ),
                (
                    locale_tag!("en"),
                    Message::Format(&[Part::Arg("tone"), Part::Lit(": "), Part::Arg("message")]),
                ),
                (
                    locale_tag!("es"),
                    Message::Format(&[Part::Arg("tone"), Part::Lit(": "), Part::Arg("message")]),
                ),
            ],
        },
        Entry {
            key: NOTIFICATIONS,
            messages: &[
                (locale_tag!("ar"), Message::Plain("الإشعارات")),
                (locale_tag!("en"), Message::Plain("Notifications")),
                (locale_tag!("es"), Message::Plain("Notificaciones")),
            ],
        },
        Entry {
            key: OPACITY,
            messages: &[
                (locale_tag!("ar"), Message::Plain("العتامة")),
                (locale_tag!("en"), Message::Plain("Opacity")),
                (locale_tag!("es"), Message::Plain("Opacidad")),
            ],
        },
        Entry {
            key: RESIZE,
            messages: &[
                (locale_tag!("ar"), Message::Plain("تغيير الحجم")),
                (locale_tag!("en"), Message::Plain("Resize")),
                (locale_tag!("es"), Message::Plain("Redimensionar")),
            ],
        },
        Entry {
            key: RESIZE_COLUMN,
            messages: &[
                (
                    locale_tag!("ar"),
                    Message::Format(&[Part::Lit("تغيير عرض العمود "), Part::Arg("label")]),
                ),
                (
                    locale_tag!("en"),
                    Message::Format(&[
                        Part::Lit("Resize "),
                        Part::Arg("label"),
                        Part::Lit(" column"),
                    ]),
                ),
                (
                    locale_tag!("es"),
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
                (locale_tag!("ar"), Message::Plain("تغيير الارتفاع")),
                (locale_tag!("en"), Message::Plain("Resize height")),
                (locale_tag!("es"), Message::Plain("Cambiar el alto")),
            ],
        },
        Entry {
            key: RESIZE_WIDTH,
            messages: &[
                (locale_tag!("ar"), Message::Plain("تغيير العرض")),
                (locale_tag!("en"), Message::Plain("Resize width")),
                (locale_tag!("es"), Message::Plain("Cambiar el ancho")),
            ],
        },
        Entry {
            key: RESTORE,
            messages: &[
                (locale_tag!("ar"), Message::Plain("استعادة")),
                (locale_tag!("en"), Message::Plain("Restore")),
                (locale_tag!("es"), Message::Plain("Restaurar")),
            ],
        },
        Entry {
            key: SATURATION,
            messages: &[
                (locale_tag!("ar"), Message::Plain("التشبع")),
                (locale_tag!("en"), Message::Plain("Saturation")),
                (locale_tag!("es"), Message::Plain("Saturación")),
            ],
        },
        Entry {
            key: SATURATION_BRIGHTNESS,
            messages: &[
                (locale_tag!("ar"), Message::Plain("التشبع والسطوع")),
                (
                    locale_tag!("en"),
                    Message::Plain("Saturation and brightness"),
                ),
                (locale_tag!("es"), Message::Plain("Saturación y brillo")),
            ],
        },
        Entry {
            key: SEARCH,
            messages: &[
                (locale_tag!("ar"), Message::Plain("بحث")),
                (locale_tag!("en"), Message::Plain("Search")),
                (locale_tag!("es"), Message::Plain("Buscar")),
            ],
        },
        Entry {
            key: SEARCH_COMMANDS,
            messages: &[
                (locale_tag!("ar"), Message::Plain("اكتب أمرًا أو ابحث…")),
                (
                    locale_tag!("en"),
                    Message::Plain("Type a command or search…"),
                ),
                (
                    locale_tag!("es"),
                    Message::Plain("Escribe un comando o busca…"),
                ),
            ],
        },
        Entry {
            key: SELECT,
            messages: &[
                (locale_tag!("ar"), Message::Plain("اختر")),
                (locale_tag!("en"), Message::Plain("Select")),
                (locale_tag!("es"), Message::Plain("Seleccionar")),
            ],
        },
        Entry {
            key: SUCCESS,
            messages: &[
                (locale_tag!("ar"), Message::Plain("نجاح")),
                (locale_tag!("en"), Message::Plain("Success")),
                (locale_tag!("es"), Message::Plain("Éxito")),
            ],
        },
        Entry {
            key: SWATCHES,
            messages: &[
                (locale_tag!("ar"), Message::Plain("عينات الألوان")),
                (locale_tag!("en"), Message::Plain("Swatches")),
                (locale_tag!("es"), Message::Plain("Muestras")),
            ],
        },
        Entry {
            key: THEN,
            messages: &[
                (
                    locale_tag!("ar"),
                    Message::Format(&[Part::Arg("first"), Part::Lit("، ثم "), Part::Arg("next")]),
                ),
                (
                    locale_tag!("en"),
                    Message::Format(&[Part::Arg("first"), Part::Lit(", then "), Part::Arg("next")]),
                ),
                (
                    locale_tag!("es"),
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
                    locale_tag!("ar"),
                    Message::Format(&[
                        Part::Arg("tone"),
                        Part::Lit(": "),
                        Part::Arg("title"),
                        Part::Lit(". "),
                        Part::Arg("message"),
                    ]),
                ),
                (
                    locale_tag!("en"),
                    Message::Format(&[
                        Part::Arg("tone"),
                        Part::Lit(": "),
                        Part::Arg("title"),
                        Part::Lit(". "),
                        Part::Arg("message"),
                    ]),
                ),
                (
                    locale_tag!("es"),
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
                (locale_tag!("ar"), Message::Plain("تحذير")),
                (locale_tag!("en"), Message::Plain("Warning")),
                (locale_tag!("es"), Message::Plain("Advertencia")),
            ],
        },
        Entry {
            key: WINDOW,
            messages: &[
                (locale_tag!("ar"), Message::Plain("النافذة")),
                (locale_tag!("en"), Message::Plain("Window")),
                (locale_tag!("es"), Message::Plain("Ventana")),
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
