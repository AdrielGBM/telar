use semantics_core::locale_tag;

use super::*;

fn reset() {
    locale().set(None);
}

#[test]
fn use_locale_is_reactive() {
    reset();
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Option<LocaleTag>>::new()));
    let s = seen.clone();
    reactive_core::effect(move || s.borrow_mut().push(use_locale()));
    set_locale(locale_tag!("en"));
    set_locale(locale_tag!("es"));
    set_locale(LocaleTag::parse("ES").unwrap());
    assert_eq!(
        *seen.borrow(),
        vec![None, Some(locale_tag!("en")), Some(locale_tag!("es"))],
        "effect re-ran on each locale switch, and not for setting the one already active"
    );
}

#[test]
fn a_surface_locale_shadows_the_thread_locale_until_cleared() {
    reset();
    set_locale(locale_tag!("en"));
    let surface = LocaleContext::new();
    surface.with_entered(|| {
        set_surface_locale(Some(&locale_tag!("es")));
        assert_eq!(current_locale(), Some(locale_tag!("es")));
        assert_eq!(use_surface_locale(), Some(locale_tag!("es")));
    });
    assert_eq!(
        current_locale(),
        Some(locale_tag!("en")),
        "the ambient world keeps the thread's locale"
    );

    set_locale(locale_tag!("de"));
    surface.with_entered(|| {
        assert_eq!(current_locale(), Some(locale_tag!("es")));
        set_surface_locale(None);
        assert_eq!(
            current_locale(),
            Some(locale_tag!("de")),
            "cleared, the surface follows the thread again"
        );
    });
    reset();
}
