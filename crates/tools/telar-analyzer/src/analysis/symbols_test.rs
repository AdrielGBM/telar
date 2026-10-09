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

const PREVIEWS: &str = "[view]\ncol\n\n[previews \"Forms/Checkbox\" layout:centered]\nA checkbox.\n\n[preview \"Default\"]\ncheckbox\n\n[preview \"Bound\" args(agree:false)]\ncheckbox checked:$agree\n\n[play]\ncanvas.click(by_role(Role::CheckBox))?;\n";

#[test]
fn the_meta_section_is_listed_by_its_title_over_its_prose() {
    let syms = document_symbols(&parse(PREVIEWS).unwrap(), PREVIEWS);
    let meta = syms
        .iter()
        .find(|s| s.name == "Forms/Checkbox")
        .expect("the [previews] meta section");
    assert_eq!(meta.kind, SymbolKind::NAMESPACE);
    assert_eq!((meta.range.start.line, meta.range.end.line), (3, 4));
    assert_eq!(meta.selection_range.start.line, 3);
}

#[test]
fn a_preview_spans_its_body_and_holds_its_play() {
    let syms = document_symbols(&parse(PREVIEWS).unwrap(), PREVIEWS);
    let default = syms.iter().find(|s| s.name == "preview: Default").unwrap();
    assert_eq!((default.range.start.line, default.range.end.line), (6, 7));
    assert!(default.children.is_none());

    let bound = syms.iter().find(|s| s.name == "preview: Bound").unwrap();
    assert_eq!((bound.range.start.line, bound.range.end.line), (9, 13));
    let play = &bound.children.as_ref().expect("its [play]")[0];
    assert_eq!(play.kind, SymbolKind::EVENT);
    assert_eq!((play.range.start.line, play.range.end.line), (12, 13));
    assert_eq!(play.selection_range.start.line, 12);
}
