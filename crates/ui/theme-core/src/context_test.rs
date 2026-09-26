use super::*;
use preferences_core::{ColorScheme, SystemPreferences, set_system_preferences};

fn scheme(color_scheme: ColorScheme) {
    set_system_preferences(SystemPreferences {
        color_scheme: Some(color_scheme),
        ..SystemPreferences::default()
    });
}

/// The whole reason this is not an `Option`: every caller that had to handle a missing theme wrote its own flat constant, and none of them followed the mode — so the careful mode-following default was unreachable on exactly the path that runs when nobody has configured anything.
#[test]
fn an_unregistered_theme_still_follows_the_mode() {
    crate::register_mode("light", || {});
    crate::register_mode("dark", || {});
    crate::follow_system("light", "dark");

    scheme(ColorScheme::Light);
    let light_ink = use_theme_tokens().ink();
    scheme(ColorScheme::Dark);
    let dark_ink = use_theme_tokens().ink();

    assert!(
        light_ink.r < 0.5,
        "dark ink on a light page, got {light_ink:?}"
    );
    assert!(
        dark_ink.r > 0.5,
        "light ink on a dark page, got {dark_ink:?}"
    );
    set_system_preferences(SystemPreferences::default());
}

#[derive(Clone)]
struct Blue;
impl ThemeTokens for Blue {
    fn primary(&self) -> Color {
        Color::rgba(0.0, 0.0, 1.0, 1.0)
    }
}

/// A theme answering one question keeps the table's answers for the rest, which is what makes `impl ThemeTokens for MyTheme {}` a valid theme.
#[test]
fn a_registered_theme_overrides_only_what_it_answers() {
    set_theme(Blue);
    assert_eq!(
        use_theme_tokens().primary(),
        Color::rgba(0.0, 0.0, 1.0, 1.0)
    );
    assert_eq!(use_theme_tokens().radius(), DefaultTokens.radius());
    THEME.with(|s| s.set(None));
}

#[derive(Clone)]
struct Red;
impl ThemeTokens for Red {
    fn primary(&self) -> Color {
        Color::rgba(1.0, 0.0, 0.0, 1.0)
    }
}

#[derive(Clone)]
struct Green;
impl ThemeTokens for Green {
    fn primary(&self) -> Color {
        Color::rgba(0.0, 1.0, 0.0, 1.0)
    }
}

const BLUE: Color = Color::rgba(0.0, 0.0, 1.0, 1.0);
const RED: Color = Color::rgba(1.0, 0.0, 0.0, 1.0);
const GREEN: Color = Color::rgba(0.0, 1.0, 0.0, 1.0);

#[test]
fn a_provided_theme_shadows_the_global_one_only_beneath_its_owner() {
    set_theme(Blue);
    let outside = reactive_core::owner_scope();
    let (inside_primary, inside_typed) = {
        let _inside = reactive_core::owner_scope();
        ScopedTheme::new(Red).provide();
        let nested = {
            let _nested = reactive_core::owner_scope();
            ScopedTheme::new(Green).provide();
            use_theme_tokens().primary()
        };
        assert_eq!(nested, GREEN, "the nearest provider wins");
        (use_theme_tokens().primary(), use_theme::<Red>().primary())
    };
    assert_eq!(inside_primary, RED);
    assert_eq!(inside_typed, RED, "use_theme resolves the same provider");
    assert_eq!(
        use_theme_tokens().primary(),
        BLUE,
        "a sibling scope still reads the global theme"
    );
    drop(outside);
    THEME.with(|s| s.set(None));
}

/// An effect built under a provider re-runs long after that build, from a write made outside every scope, and must still resolve the provider rather than whatever is ambient when it runs.
#[test]
fn a_re_run_under_a_provider_keeps_reading_the_provider() {
    use std::cell::RefCell;

    set_theme(Blue);
    let tick = signal(0u32);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let scoped = {
        let _scope = reactive_core::owner_scope();
        let scoped = ScopedTheme::new(Red);
        scoped.provide();
        let seen = Rc::clone(&seen);
        reactive_core::effect(move || {
            tick.get();
            seen.borrow_mut().push(use_theme_tokens().primary());
        });
        scoped
    };

    tick.set(1);
    scoped.set(Green);
    set_theme(Red);

    assert_eq!(
        *seen.borrow(),
        vec![RED, RED, GREEN],
        "built, re-run by an outside write, re-run by its own theme, and never by the global one"
    );
    THEME.with(|s| s.set(None));
}

#[test]
fn a_handler_run_under_its_owner_reads_the_provider() {
    set_theme(Blue);
    let owner = {
        let scope = reactive_core::owner_scope();
        ScopedTheme::new(Red).provide();
        scope.id()
    };
    assert_eq!(use_theme_tokens().primary(), BLUE);
    assert_eq!(
        reactive_core::with_owner(Some(owner), || use_theme_tokens().primary()),
        RED
    );
    THEME.with(|s| s.set(None));
}

/// A library's widgets under their own theme inside an application with its own: each reader asks for its own type and gets the nearest one, whatever sits between.
#[test]
fn a_typed_read_walks_past_a_provider_of_another_theme_type() {
    set_theme(Blue);
    let _app = reactive_core::owner_scope();
    ScopedTheme::new(Red).provide();
    let _library = reactive_core::owner_scope();
    ScopedTheme::new(Green).provide();

    assert_eq!(use_theme::<Green>().primary(), GREEN);
    assert_eq!(
        use_theme::<Red>().primary(),
        RED,
        "the outer Red, past the Green in between"
    );
    assert_eq!(
        use_theme::<Blue>().primary(),
        BLUE,
        "the global theme when no provider holds one"
    );
    assert_eq!(
        use_theme_tokens().primary(),
        GREEN,
        "the untyped read still takes the nearest provider"
    );
    THEME.with(|s| s.set(None));
}

#[derive(Clone)]
struct Tinted(Color);
impl ThemeTokens for Tinted {}

#[test]
fn a_provider_switched_to_the_read_type_takes_over_the_read() {
    use std::cell::RefCell;

    let seen = Rc::new(RefCell::new(Vec::new()));
    let _app = reactive_core::owner_scope();
    ScopedTheme::new(Tinted(RED)).provide();
    let library = {
        let _library = reactive_core::owner_scope();
        let library = ScopedTheme::new(Green);
        library.provide();
        let seen = Rc::clone(&seen);
        reactive_core::effect(move || seen.borrow_mut().push(use_theme::<Tinted>().0));
        library
    };

    library.set(Tinted(BLUE));
    assert_eq!(*seen.borrow(), vec![RED, BLUE]);
}

#[test]
#[should_panic(expected = "found no theme of that type")]
fn a_typed_read_with_no_such_theme_anywhere_names_the_type() {
    set_theme(Blue);
    let _app = reactive_core::owner_scope();
    ScopedTheme::new(Green).provide();
    let _ = use_theme::<Red>();
}
