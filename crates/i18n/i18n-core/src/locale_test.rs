use super::*;

fn reset() {
    LOCALE.with(|s| s.set(None));
}

#[test]
fn use_locale_is_reactive() {
    reset();
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Option<String>>::new()));
    let s = seen.clone();
    let _e = reactive_core::effect(move || s.borrow_mut().push(use_locale()));
    set_locale("en");
    set_locale("es");
    assert_eq!(
        *seen.borrow(),
        vec![None, Some("en".into()), Some("es".into())],
        "effect re-ran on each locale switch"
    );
}
