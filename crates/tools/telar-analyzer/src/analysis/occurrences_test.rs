use super::*;

const SRC: &str = "[style]\n@card\n    width: 240\n[view]\ncol @card\n    box @card\n";

#[test]
fn class_at_recognizes_def_and_ref() {
    assert_eq!(class_at(SRC, 1, 2).as_deref(), Some("card"));
    assert_eq!(class_at(SRC, 4, 6).as_deref(), Some("card"));
    assert_eq!(class_at(SRC, 2, 5), None);
}

#[test]
fn occurrences_cover_def_and_all_refs() {
    let ranges = class_occurrences(SRC, "card");
    let lines: Vec<u32> = ranges.iter().map(|r| r.start.line).collect();
    assert_eq!(lines, vec![1, 4, 5]);
    for r in &ranges {
        assert_eq!(r.end.character - r.start.character, 4);
    }
}

#[test]
fn component_at_recognizes_non_builtin_tags_only() {
    let src = "[view]\ncol\n    feature_card icon:\"x\"\n    text \"hi\"\n";
    assert_eq!(component_at(src, 2, 6).as_deref(), Some("feature_card"));
    assert_eq!(component_at(src, 1, 0), None);
    assert_eq!(component_at(src, 3, 4), None);
}

#[test]
fn signals_resolve_across_logic_and_view() {
    let src =
        "[logic]\nlet count = signal(0i32);\nlet x = 5;\n[view]\ncol\n    text \"{$count}\"\n";
    assert_eq!(signal_at(src, 1, 5).as_deref(), Some("count"));
    assert_eq!(signal_at(src, 5, 13).as_deref(), Some("count"));
    assert_eq!(signal_at(src, 2, 4), None);
    let lines: Vec<u32> = signal_occurrences(src, "count")
        .iter()
        .map(|r| r.start.line)
        .collect();
    assert!(lines.contains(&1) && lines.contains(&5), "lines: {lines:?}");
}

#[test]
fn declared_signals_lists_logic_signals() {
    let src =
        "[logic]\nlet count = signal(0i32);\nlet double = memo(|| 0);\nlet x = 5;\n[view]\ncol\n";
    let mut names = declared_signals(src);
    names.sort();
    assert_eq!(names, vec!["count".to_string(), "double".to_string()]);
}

#[test]
fn logic_at_signs_are_ignored() {
    let src = "[logic]\nlet x = foo(\"@card\");\n[view]\ncol @card\n";
    let ranges = class_occurrences(src, "card");
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].start.line, 3);
}
