//! What the catalogue says by itself, with no application catalog installed.

mod strings_support;

use strings_support::*;
use telar::set_locale;

#[test]
fn the_select_placeholder_is_english_by_default() {
    set_locale("en");
    assert!(select_placeholder().contains(&"Select".to_string()));
}

#[test]
fn the_select_placeholder_follows_the_locale() {
    set_locale("es");
    assert!(select_placeholder().contains(&"Seleccionar".to_string()));
    set_locale("ar");
    assert!(select_placeholder().contains(&"اختر".to_string()));
}

#[cfg(feature = "overlays")]
mod modal {
    use super::*;
    use telar::signal;
    use telar_components::ModalProps;

    fn props() -> ModalProps {
        ModalProps::props()
            .open(signal(true))
            .title("Confirm")
            .build()
    }

    #[test]
    fn close_is_english_by_default() {
        set_locale("en");
        assert!(open_modal(props()).contains(&"Close".to_string()));
    }

    #[test]
    fn close_follows_the_locale() {
        set_locale("es");
        assert!(open_modal(props()).contains(&"Cerrar".to_string()));
        set_locale("ar");
        assert!(open_modal(props()).contains(&"إغلاق".to_string()));
    }

    #[test]
    fn the_close_label_prop_wins() {
        set_locale("es");
        let props = ModalProps::props()
            .open(signal(true))
            .title("Confirm")
            .close_label("Done")
            .build();
        let drawn = open_modal(props);
        assert!(drawn.contains(&"Done".to_string()));
        assert!(!drawn.contains(&"Cerrar".to_string()));
    }
}
