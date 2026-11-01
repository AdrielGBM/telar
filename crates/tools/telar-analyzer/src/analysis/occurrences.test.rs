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
    let signal = signal_at(src, 1, 5).unwrap();
    assert_eq!(signal.name, "count");
    assert_eq!(signal_at(src, 5, 13), Some(signal.clone()));
    assert_eq!(signal_at(src, 2, 4), None);
    let lines: Vec<u32> = signal_occurrences(src, &signal)
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

const PREVIEWS: &str = "[logic]\nlet on = signal(false);\n[view]\ncol\n    text \"{$on}\"\n[preview \"A\" args(on:true)]\ncol\n    text \"{$on}\"\n    text \"{$on}\"\n[preview \"B\" args(other:1)]\ncol\n    text \"{$on}\"\n";

fn lines(ranges: &[Range]) -> Vec<u32> {
    ranges.iter().map(|range| range.start.line).collect()
}

#[test]
fn an_arg_is_local_to_its_preview() {
    let signal = signal_at(PREVIEWS, 7, 14).expect("a read of an arg");
    let ranges = signal_occurrences(PREVIEWS, &signal);
    assert_eq!(lines(&ranges), vec![5, 7, 8]);
    assert_eq!(ranges[0].start.character, 18);
}

#[test]
fn the_declaration_in_the_header_is_the_same_arg() {
    let from_header = signal_at(PREVIEWS, 5, 19).expect("the declaration");
    assert_eq!(from_header, signal_at(PREVIEWS, 8, 14).unwrap());
    let range = signal_occurrence_at(PREVIEWS, 5, 19).unwrap();
    assert_eq!(range.start.line, 5);
    assert_eq!(range.end.character - range.start.character, 2);
}

#[test]
fn a_logic_signal_skips_the_previews_that_shadow_it() {
    let signal = signal_at(PREVIEWS, 4, 14).expect("a read of the logic signal");
    assert_eq!(
        lines(&signal_occurrences(PREVIEWS, &signal)),
        vec![1, 4, 11]
    );
    assert_eq!(signal_at(PREVIEWS, 11, 14), Some(signal));
}

#[test]
fn a_read_of_an_undeclared_name_is_not_a_signal() {
    let src = "[preview \"A\" args(on:true)]\ncol\n    text \"{$nope}\"\n";
    assert_eq!(signal_at(src, 2, 14), None);
}
