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

#[test]
fn hot_rewrite_hands_the_macro_the_type_the_author_wrote() {
    let rewrite = |line: &str| hot_rewrite_signal_decl(line, "c").unwrap();
    assert_eq!(
        rewrite("let open: RwSignal<Option<Open>> = signal(None);"),
        "let open: RwSignal<Option<Open>> = telar::hot_signal_auto!(\"c::open\", type RwSignal<Option<Open>>, None);"
    );
    assert_eq!(
        rewrite("    let mut rows: telar::RwSignal<Vec<(u8, Option<Row>)>> = signal(Vec::new());"),
        "    let mut rows: telar::RwSignal<Vec<(u8, Option<Row>)>> = telar::hot_signal_auto!(\"c::rows\", type telar::RwSignal<Vec<(u8, Option<Row>)>>, Vec::new());"
    );
    assert_eq!(
        rewrite("let it: RwSignal<Box<dyn Iterator<Item = u8>>> = signal(empty());"),
        "let it: RwSignal<Box<dyn Iterator<Item = u8>>> = telar::hot_signal_auto!(\"c::it\", type RwSignal<Box<dyn Iterator<Item = u8>>>, empty());"
    );
    assert_eq!(
        rewrite("let f: RwSignal<Box<dyn Fn() -> u8>> = signal(Box::new(|| 1));"),
        "let f: RwSignal<Box<dyn Fn() -> u8>> = telar::hot_signal_auto!(\"c::f\", type RwSignal<Box<dyn Fn() -> u8>>, Box::new(|| 1));"
    );
    assert_eq!(
        rewrite("let r: ReadSignal<u8> = signal(0);"),
        "let r: ReadSignal<u8> = telar::hot_signal_auto!(\"c::r\", type ReadSignal<u8>, 0);",
        "passed through as written, so a type `signal` cannot return fails in every flavour alike"
    );
}

#[test]
fn hot_rewrite_reads_the_type_from_a_turbofish() {
    let rewrite = |line: &str| hot_rewrite_signal_decl(line, "c").unwrap();
    assert_eq!(
        rewrite("let open = signal::<Option<Open>>(None);"),
        "let open = telar::hot_signal_auto!(\"c::open\", type telar::RwSignal<Option<Open>>, None);"
    );
    assert_eq!(
        rewrite("let f = signal::<Box<dyn Fn() -> u8>>(\n"),
        "let f = telar::hot_signal_auto!(\"c::f\", type telar::RwSignal<Box<dyn Fn() -> u8>>, \n"
    );
}

#[test]
fn hot_rewrite_leaves_an_unannotated_signal_untyped() {
    assert_eq!(
        hot_rewrite_signal_decl("let status = signal(", "c").unwrap(),
        "let status = telar::hot_signal_auto!(\"c::status\", "
    );
}
