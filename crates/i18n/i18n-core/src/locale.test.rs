use super::*;

fn reset() {
    locale().set(None);
}

#[test]
fn use_locale_is_reactive() {
    reset();
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Option<String>>::new()));
    let s = seen.clone();
    let _e = reactive_core::effect(move || s.borrow_mut().push(use_locale()));
    set_locale("en");
    set_locale("es");
    set_locale("es");
    assert_eq!(
        *seen.borrow(),
        vec![None, Some("en".into()), Some("es".into())],
        "effect re-ran on each locale switch, and not for setting the one already active"
    );
}

#[test]
fn a_surface_locale_shadows_the_thread_locale_until_cleared() {
    reset();
    set_locale("en");
    let surface = LocaleContext::new();
    surface.with_entered(|| {
        set_surface_locale(Some("es"));
        assert_eq!(current_locale().as_deref(), Some("es"));
        assert_eq!(use_surface_locale().as_deref(), Some("es"));
    });
    assert_eq!(
        current_locale().as_deref(),
        Some("en"),
        "the ambient world keeps the thread's locale"
    );

    set_locale("de");
    surface.with_entered(|| {
        assert_eq!(current_locale().as_deref(), Some("es"));
        set_surface_locale(None);
        assert_eq!(
            current_locale().as_deref(),
            Some("de"),
            "cleared, the surface follows the thread again"
        );
    });
    reset();
}
