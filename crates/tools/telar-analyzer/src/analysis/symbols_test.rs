use super::*;
use telar_parser::parse;

#[test]
fn outlines_classes_and_previews_in_order() {
    let src =
        "[style]\n@card\n    width: 240\n\n[view]\ncol @card\n\n[preview \"Default\"]\ncard\n";
    let doc = parse(src).unwrap();
    let syms = document_symbols(&doc, src);
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"@card"), "class: {names:?}");
    assert!(
        names.iter().any(|n| n.contains("Default")),
        "preview: {names:?}"
    );
    let lines: Vec<u32> = syms.iter().map(|s| s.range.start.line).collect();
    assert!(lines.windows(2).all(|w| w[0] <= w[1]), "sorted: {lines:?}");

    let card = syms.iter().find(|s| s.name == "@card").unwrap();
    assert_eq!(card.kind, SymbolKind::CLASS);
}
