/// One [`Entry`](crate::Entry) whose only message is English, for a crate that ships its strings in English and leaves every other locale to the application's catalog.
///
/// A plain string is a [`Message::Plain`](crate::Message::Plain); literals and `{name}` placeholders in sequence are a [`Message::Format`](crate::Message::Format), filled by name at lookup.
///
/// ```
/// use telar_i18n_core::{Catalog, english, translate};
///
/// static CATALOG: Catalog = Catalog {
///     locales: &["en"],
///     default_locale: "en",
///     entries: &[
///         english!("close", "Close"),
///         english!("fps", { count }, " fps"),
///         english!("reset", "Reset ", { name }),
///     ],
/// };
///
/// assert_eq!(translate(&CATALOG, "close", &[]), "Close");
/// assert_eq!(translate(&CATALOG, "fps", &[("count", "60")]), "60 fps");
/// assert_eq!(translate(&CATALOG, "reset", &[("name", "label")]), "Reset label");
/// ```
#[macro_export]
macro_rules! english {
    (@part { $name:ident }) => {
        $crate::Part::Arg(stringify!($name))
    };
    (@part $text:literal) => {
        $crate::Part::Lit($text)
    };
    ($key:expr, $text:literal $(,)?) => {
        $crate::Entry {
            key: $key,
            messages: &[("en", $crate::Message::Plain($text))],
        }
    };
    ($key:expr, $($part:tt),+ $(,)?) => {
        $crate::Entry {
            key: $key,
            messages: &[(
                "en",
                $crate::Message::Format(&[$($crate::english!(@part $part)),+]),
            )],
        }
    };
}

#[cfg(test)]
#[path = "english_test.rs"]
mod tests;
