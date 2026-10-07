use super::*;

#[test]
fn snapshot_and_restore_roundtrip() {
    let a = hot_signal("t::a", 1i32);
    let b = hot_signal("t::b", String::from("hi"));
    a.set(41);
    b.set("hola".to_string());
    let blob = hot_snapshot_json();

    hot_restore_json(&blob);
    let a2 = hot_signal("t::a", 0i32);
    let b2 = hot_signal("t::b", String::new());
    assert_eq!(a2.peek(), 41);
    assert_eq!(b2.peek(), "hola");
}

#[test]
fn missing_or_corrupt_values_fall_back_to_init() {
    hot_restore_json("{\"t2::x\": \"not-an-int\"}");
    let x = hot_signal("t2::x", 7i32);
    assert_eq!(x.peek(), 7);
    let y = hot_signal("t2::y", 3i32);
    assert_eq!(y.peek(), 3);
}

#[test]
fn theme_mode_survives_snapshot_restore() {
    theme_core::register_mode("midnight", || {});
    theme_core::set_mode("midnight");
    let blob = hot_snapshot_json();

    theme_core::register_mode("modern", || {});
    theme_core::set_mode("modern");
    hot_restore_json(&blob);
    assert_eq!(theme_core::active_mode().as_deref(), Some("midnight"));
}

#[test]
fn auto_macro_falls_back_for_non_serde_types() {
    struct NotSerde(i32);
    let plain = crate::hot_signal_auto!("t3::plain", NotSerde(5));
    plain.update(|v| v.0 += 1);
    assert_eq!(plain.with(|v| v.0), 6);

    let kept = crate::hot_signal_auto!("t3::kept", 9i32);
    assert_eq!(kept.peek(), 9);
    assert!(
        hot_snapshot_json().contains("t3::kept"),
        "a non-serde type still reaches the snapshot: {}",
        hot_snapshot_json()
    );
}

#[test]
fn a_chosen_scheme_survives_snapshot_restore() {
    theme_core::set_scheme_preference(theme_core::SchemePreference::Dark);
    let blob = hot_snapshot_json();
    theme_core::set_scheme_preference(theme_core::SchemePreference::System);
    hot_restore_json(&blob);
    assert_eq!(
        theme_core::scheme_preference(),
        theme_core::SchemePreference::Dark
    );
}

#[test]
fn a_reduced_motion_override_survives_snapshot_restore() {
    preferences_core::set_reduced_motion_override(Some(true));
    let blob = hot_snapshot_json();
    preferences_core::set_reduced_motion_override(None);
    hot_restore_json(&blob);
    assert_eq!(preferences_core::reduced_motion_override(), Some(true));
}

#[test]
fn following_the_system_adds_nothing_to_the_snapshot() {
    preferences_core::set_reduced_motion_override(None);
    assert!(!hot_snapshot_json().contains(REDUCED_MOTION_OVERRIDE_KEY));
}

/// The transpiler swaps `signal(` for this macro and keeps the author's arguments as written, so a formatter's trailing comma has to be as welcome here as in the call it replaced.
#[test]
fn a_trailing_comma_is_accepted_like_the_call_it_replaces() {
    let s = crate::hot_signal_auto!("t4::trailing", 1i32 + 1,);
    assert_eq!(s.peek(), 2);
}

/// `None` alone is an `Option<_>`, and the probe settles on serde before the annotation can say what fills it; written in, the type picks the plain signal a non-serde value needs.
#[test]
fn a_named_signal_type_decides_serde_before_the_value_is_probed() {
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum NotSerde {
        A,
    }
    let open: RwSignal<Option<NotSerde>> =
        crate::hot_signal_auto!("t5::open", type RwSignal<Option<NotSerde>>, None);
    open.set(Some(NotSerde::A));
    assert_eq!(open.peek(), Some(NotSerde::A));

    let nested = crate::hot_signal_auto!(
        "t5::nested",
        type reactive_core::RwSignal<Vec<(u8, Option<String>)>>,
        Vec::new(),
    );
    assert!(nested.peek().is_empty());
    assert!(hot_snapshot_json().contains("t5::nested"));

    let inferred: RwSignal<u8> = crate::hot_signal_auto!("t5::inferred", type RwSignal<_>, 3);
    assert_eq!(inferred.peek(), 3);
}
