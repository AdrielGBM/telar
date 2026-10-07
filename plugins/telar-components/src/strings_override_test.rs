//! An application catalog overriding what the catalogue says. Its own binary because the installed catalog is process-wide.

mod strings_support;

use strings_support::*;
use telar::i18n::{Catalog, Entry, Message};
use telar::{set_catalog, set_locale};

static APP: Catalog = Catalog {
    locales: &["en", "es"],
    default_locale: "en",
    entries: &[
        Entry {
            key: "telar_components.close",
            messages: &[
                ("en", Message::Plain("Dismiss")),
                ("es", Message::Plain("Descartar")),
            ],
        },
        Entry {
            key: "telar_components.select",
            messages: &[("en", Message::Plain("Pick one"))],
        },
    ],
};

#[test]
fn the_app_overrides_the_select_placeholder() {
    set_catalog(&APP);
    set_locale("en");
    assert!(select_placeholder().contains(&"Pick one".to_string()));
}

#[test]
fn a_locale_the_app_does_not_translate_keeps_the_plugin_text() {
    set_catalog(&APP);
    set_locale("es");
    assert!(select_placeholder().contains(&"Seleccionar".to_string()));
}

#[cfg(feature = "overlays")]
mod modal {
    use super::*;
    use telar::signal;
    use telar_components::ModalProps;

    #[test]
    fn the_app_overrides_close() {
        set_catalog(&APP);
        set_locale("en");
        let props = ModalProps::props()
            .open(signal(true))
            .title("Confirm")
            .build();
        assert!(open_modal(props).contains(&"Dismiss".to_string()));
        set_locale("es");
        let props = ModalProps::props()
            .open(signal(true))
            .title("Confirm")
            .build();
        assert!(open_modal(props).contains(&"Descartar".to_string()));
    }

    #[test]
    fn the_close_label_prop_beats_the_app() {
        set_catalog(&APP);
        set_locale("en");
        let props = ModalProps::props()
            .open(signal(true))
            .title("Confirm")
            .close_label("Done")
            .build();
        let drawn = open_modal(props);
        assert!(drawn.contains(&"Done".to_string()));
        assert!(!drawn.contains(&"Dismiss".to_string()));
    }
}
