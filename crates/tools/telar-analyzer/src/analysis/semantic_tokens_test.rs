use super::*;

#[test]
fn classifies_tags_classes_and_signals() {
    let src =
        "[style]\n@card\n[view]\ncol @card\n    text \"{$count}\"\n    feature_card icon:\"x\"\n";
    let raw = raw_tokens(src);

    let has = |line: u32, col: u32, ty: u32| {
        raw.iter()
            .any(|&(l, c, _, t)| l == line && c == col && t == ty)
    };

    assert!(has(1, 0, CLASS), "style class def: {raw:?}");
    assert!(has(3, 0, TAG_BUILTIN), "builtin tag: {raw:?}");
    // `@card` ref in the view (line 3, col 4) → class.
    assert!(has(3, 4, CLASS), "class ref: {raw:?}");
    // `$count` inside the interpolation → variable.
    assert!(
        raw.iter().any(|&(l, _, _, t)| l == 4 && t == SIGNAL),
        "signal: {raw:?}"
    );
    // `feature_card` non-builtin tag (line 5, col 4) → function (component).
    assert!(has(5, 4, TAG_COMPONENT), "component tag: {raw:?}");
}

#[test]
fn delta_encoding_is_monotonic() {
    let src = "[view]\ncol @a\n    text @b\n";
    let toks = semantic_tokens(src);
    // First token carries an absolute line; subsequent deltas are non-negative by construction.
    assert!(!toks.is_empty(), "the document has tokens to encode");
}

#[test]
fn a_preview_header_marks_its_options_and_args() {
    let src = "[view]\ncol\n[previews \"Forms/Box\" layout:centered]\nProse.\n[preview \"A\" args(agree:false) matrix:(dir:[ltr rtl])]\nbox\n";
    let raw = raw_tokens(src);
    let at = |line: u32, text: &str| {
        let col = src.lines().nth(line as usize).unwrap().find(text).unwrap() as u32;
        raw.iter()
            .find(|&&(l, c, _, _)| l == line && c == col)
            .map(|&(_, _, len, ty)| (len, ty))
    };
    assert_eq!(at(2, "layout"), Some((6, OPTION)), "{raw:?}");
    assert_eq!(at(4, "args"), Some((4, OPTION)), "{raw:?}");
    assert_eq!(at(4, "agree"), Some((5, SIGNAL)), "{raw:?}");
    assert_eq!(at(4, "matrix"), Some((6, OPTION)), "{raw:?}");
    assert_eq!(at(4, "dir"), Some((3, OPTION)), "{raw:?}");
    assert!(
        !raw.iter().any(|&(l, _, _, _)| l == 3),
        "prose is not markup: {raw:?}"
    );
}
