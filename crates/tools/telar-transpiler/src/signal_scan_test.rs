use super::*;

#[test]
fn hot_rewrite_keys_signal_binding() {
    let out = hot_rewrite_signal_decl("let count = signal(0i32);", "counter").unwrap();
    assert_eq!(
        out,
        "let count = telar::hot_signal_auto!(\"counter::count\", 0i32);"
    );
}

#[test]
fn hot_rewrite_skips_memos_and_plain_lets() {
    assert!(
        hot_rewrite_signal_decl("let d = memo(move || 1);", "c").is_none(),
        "a memo is not a signal declaration"
    );
    assert!(
        hot_rewrite_signal_decl("let x = 5;", "c").is_none(),
        "nor is a plain let"
    );
    assert!(
        hot_rewrite_signal_decl("count.set(signal_like);", "c").is_none(),
        "nor is a call that merely mentions one"
    );
}

#[test]
fn hot_rewrite_preserves_nested_parens_and_mut() {
    let out = hot_rewrite_signal_decl("let mut v = signal(vec![(1, 2)]);", "grid").unwrap();
    assert_eq!(
        out,
        "let mut v = telar::hot_signal_auto!(\"grid::v\", vec![(1, 2)]);"
    );
}

#[test]
fn detects_rw_signal() {
    let s = scan_signals("let count = signal(0i32);");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].name, "count");
    assert_eq!(s[0].kind, SignalKind::RwSignal);
}

#[test]
fn detects_memo() {
    let s = scan_signals("let double = memo(move |_| count.get() * 2);");
    assert_eq!(s[0].name, "double");
    assert_eq!(s[0].kind, SignalKind::Memo);
}

#[test]
fn ignores_plain_let() {
    let s = scan_signals("let x = 5;");
    assert!(s.is_empty(), "a plain let declares no signal: {s:?}");
}
