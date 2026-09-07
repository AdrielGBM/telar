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
