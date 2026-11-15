use telar_i18n_core::{
    Catalog, Entry, Message, Part, set_catalog, set_locale, translate_with_override,
};
use telar_i18n_core::{LocaleTag, locale_tag};

static PLUGIN: Catalog = Catalog {
    locales: &[locale_tag!("en"), locale_tag!("es")],
    default_locale: locale_tag!("en"),
    entries: &[
        Entry {
            key: "close",
            messages: &[
                (locale_tag!("en"), Message::Plain("Close")),
                (locale_tag!("es"), Message::Plain("Cerrar")),
            ],
        },
        Entry {
            key: "hello",
            messages: &[
                (
                    locale_tag!("en"),
                    Message::Format(&[Part::Lit("Hello, "), Part::Arg("name")]),
                ),
                (
                    locale_tag!("es"),
                    Message::Format(&[Part::Lit("Hola, "), Part::Arg("name")]),
                ),
            ],
        },
        Entry {
            key: "open",
            messages: &[
                (locale_tag!("en"), Message::Plain("Open")),
                (locale_tag!("es"), Message::Plain("Abrir")),
            ],
        },
    ],
};

static APP: Catalog = Catalog {
    locales: &[locale_tag!("en"), locale_tag!("es"), locale_tag!("fr")],
    default_locale: locale_tag!("en"),
    entries: &[
        Entry {
            key: "ui.close",
            messages: &[(locale_tag!("en"), Message::Plain("Dismiss"))],
        },
        Entry {
            key: "ui.hello",
            messages: &[
                (
                    locale_tag!("en"),
                    Message::Format(&[Part::Lit("Hi, "), Part::Arg("name")]),
                ),
                (
                    locale_tag!("es"),
                    Message::Format(&[Part::Lit("Que tal, "), Part::Arg("name")]),
                ),
            ],
        },
    ],
};

fn install_app_catalog() {
    set_catalog(&APP);
}

fn tr(namespace: &str, key: &str, args: &[(&str, &str)]) -> String {
    translate_with_override(namespace, &PLUGIN, key, args)
}

#[test]
fn an_app_message_overrides_the_plugin_text() {
    install_app_catalog();
    set_locale(tag("en"));
    assert_eq!(tr("ui", "close", &[]), "Dismiss");
}

#[test]
fn a_key_the_app_does_not_override_uses_the_plugin_text() {
    install_app_catalog();
    set_locale(tag("es"));
    assert_eq!(tr("ui", "open", &[]), "Abrir");
    set_locale(tag("en"));
    assert_eq!(tr("ui", "open", &[]), "Open");
}

#[test]
fn another_namespace_is_not_overridden() {
    install_app_catalog();
    set_locale(tag("en"));
    assert_eq!(tr("other", "close", &[]), "Close");
}

#[test]
fn args_render_in_both_the_override_and_the_plugin_text() {
    install_app_catalog();
    set_locale(tag("en"));
    assert_eq!(tr("ui", "hello", &[("name", "Ada")]), "Hi, Ada");
    assert_eq!(tr("other", "hello", &[("name", "Ada")]), "Hello, Ada");
}

#[test]
fn the_active_locale_follows_a_language_switch() {
    install_app_catalog();
    set_locale(tag("es"));
    assert_eq!(tr("ui", "hello", &[("name", "Ada")]), "Que tal, Ada");
    set_locale(tag("en"));
    assert_eq!(tr("ui", "hello", &[("name", "Ada")]), "Hi, Ada");
}

#[test]
fn the_plugin_active_locale_beats_the_app_default_locale() {
    install_app_catalog();
    set_locale(tag("es"));
    assert_eq!(tr("ui", "close", &[]), "Cerrar");
}

#[test]
fn the_app_default_locale_beats_the_plugin_default_locale() {
    install_app_catalog();
    set_locale(tag("fr"));
    assert_eq!(tr("ui", "close", &[]), "Dismiss");
    assert_eq!(tr("ui", "open", &[]), "Open");
}

#[test]
fn an_unset_locale_uses_the_app_default_locale() {
    install_app_catalog();
    std::thread::spawn(|| {
        assert_eq!(tr("ui", "close", &[]), "Dismiss");
    })
    .join()
    .unwrap();
}

#[test]
fn a_key_absent_everywhere_renders_as_the_key() {
    install_app_catalog();
    set_locale(tag("en"));
    assert_eq!(tr("ui", "nope", &[]), "nope");
}

fn tag(text: &str) -> LocaleTag {
    LocaleTag::parse(text).unwrap()
}
