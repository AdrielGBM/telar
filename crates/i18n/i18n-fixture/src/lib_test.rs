// The whole i18n pipeline end to end: the baker turned `locales/*.toml` into a catalog, `t!` validated its keys and args at compile time, and `translate` renders the active locale.
#[test]
fn catalog_translates_and_switches() {
    telar::set_locale("en");
    assert_eq!(telar::t!("greeting", name = "Ada"), "Hello, Ada!");
    assert_eq!(telar::t!("nav.overview"), "Overview");
    telar::set_locale("es");
    assert_eq!(telar::t!("greeting", name = "Ada"), "Hola, Ada!");
    assert_eq!(telar::t!("nav.overview"), "Resumen");
    telar::set_locale("fr");
    assert_eq!(telar::t!("nav.overview"), "Overview");
}

// A plural table is baked as one key with per-category branches, and the active locale's rules pick one.
#[test]
fn plural_selects_a_branch_per_locale() {
    telar::set_locale("en");
    assert_eq!(telar::t!("items", count = "1"), "1 item");
    assert_eq!(telar::t!("items", count = "0"), "0 items");
    assert_eq!(telar::t!("items", count = "5"), "5 items");

    telar::set_locale("es");
    assert_eq!(telar::t!("items", count = "1"), "1 elemento");
    assert_eq!(telar::t!("items", count = "5"), "5 elementos");

    // Arabic is the reason the category set is not just one/other: it uses all six.
    telar::set_locale("ar");
    assert_eq!(telar::t!("items", count = "0"), "لا عناصر");
    assert_eq!(telar::t!("items", count = "1"), "عنصر واحد");
    assert_eq!(telar::t!("items", count = "2"), "عنصران");
    assert_eq!(telar::t!("items", count = "3"), "3 عناصر");
    assert_eq!(telar::t!("items", count = "11"), "11 عنصرًا");
    telar::set_locale("en");
}

mod plugin_override {
    use telar::i18n::{Catalog, Entry, Message, translate_with_override};
    use telar::testing::{mount, texts};
    use telar::{Color, LayoutStyle, Text, TextStyle, WindowRoot};

    // The catalog `telar-components` ships its "Close" in, which a core crate may not depend on.
    static COMPONENTS: Catalog = Catalog {
        locales: &["ar", "en", "es"],
        default_locale: "en",
        entries: &[Entry {
            key: "close",
            messages: &[
                ("ar", Message::Plain("إغلاق")),
                ("en", Message::Plain("Close")),
                ("es", Message::Plain("Cerrar")),
            ],
        }],
    };

    fn drawn_close_label() -> Vec<String> {
        telar::reset_layout_runtime();
        let label = Text::new(
            || translate_with_override("telar_components", &COMPONENTS, "close", &[]),
            LayoutStyle::new(),
            || TextStyle::new(14.0, Color::BLACK),
        )
        .expect("the label lays out");
        texts(&mount(WindowRoot::new(Box::new(label)), 320, 80))
    }

    // `rsx_modules!` installed the catalog it baked as this binary loaded, so the override reaches a plugin string with no call to install it.
    #[test]
    fn the_baked_catalog_overrides_a_plugin_string_without_being_installed_by_hand() {
        let installed = telar::i18n::catalog().expect("the application catalog is installed");
        assert!(std::ptr::eq(installed, &crate::__rsx_i18n::CATALOG));

        telar::set_locale("en");
        assert_eq!(drawn_close_label(), ["Dismiss"]);
        telar::set_locale("es");
        assert_eq!(drawn_close_label(), ["Descartar"]);
        telar::set_locale("ar");
        assert_eq!(drawn_close_label(), ["إغلاق"]);
    }
}
