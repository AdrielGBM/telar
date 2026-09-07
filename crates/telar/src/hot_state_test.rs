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
