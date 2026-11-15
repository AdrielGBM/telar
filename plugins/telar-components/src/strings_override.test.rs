//! An application catalog overriding what the catalogue says. Its own binary because the installed catalog is process-wide.

#[path = "strings_support.test.rs"]
mod strings_support;

use strings_support::*;
use telar::i18n::{Catalog, Entry, Message};
use telar::{set_catalog, set_locale};

static APP: Catalog = Catalog {
    locales: &[telar::locale_tag!("en"), telar::locale_tag!("es")],
    default_locale: telar::locale_tag!("en"),
    entries: &[
        Entry {
            key: "telar_components.close",
            messages: &[
                (telar::locale_tag!("en"), Message::Plain("Dismiss")),
                (telar::locale_tag!("es"), Message::Plain("Descartar")),
            ],
        },
        Entry {
            key: "telar_components.select",
            messages: &[(telar::locale_tag!("en"), Message::Plain("Pick one"))],
        },
    ],
};

#[test]
fn the_app_overrides_the_select_placeholder() {
    set_catalog(&APP);
    set_locale(telar::locale_tag!("en"));
    assert!(select_placeholder().contains(&"Pick one".to_string()));
}

#[test]
fn a_locale_the_app_does_not_translate_keeps_the_plugin_text() {
    set_catalog(&APP);
    set_locale(telar::locale_tag!("es"));
    assert!(select_placeholder().contains(&"Seleccionar".to_string()));
}

mod modal {
    use super::*;
    use telar::signal;
    use telar_components::ModalProps;

    #[test]
    fn the_app_overrides_close() {
        set_catalog(&APP);
        set_locale(telar::locale_tag!("en"));
        let props = ModalProps::props()
            .control(signal(true))
            .title("Confirm")
            .build();
        assert!(open_modal(props).contains(&"Dismiss".to_string()));
        set_locale(telar::locale_tag!("es"));
        let props = ModalProps::props()
            .control(signal(true))
            .title("Confirm")
            .build();
        assert!(open_modal(props).contains(&"Descartar".to_string()));
    }

    #[test]
    fn the_close_label_prop_beats_the_app() {
        set_catalog(&APP);
        set_locale(telar::locale_tag!("en"));
        let props = ModalProps::props()
            .control(signal(true))
            .title("Confirm")
            .close_label("Done")
            .build();
        let drawn = open_modal(props);
        assert!(drawn.contains(&"Done".to_string()));
        assert!(!drawn.contains(&"Dismiss".to_string()));
    }
}
