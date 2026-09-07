use super::*;
use std::str::FromStr;
use telar_parser::parse;

#[test]
fn one_lens_per_preview_on_its_header_line() {
    let src = "[view]\ncol\n\n[preview \"A\"]\ncol\n\n[preview \"B\"]\nbox\n";
    let doc = parse(src).unwrap();
    let uri = Uri::from_str("file:///x.rsx").unwrap();
    let lenses = code_lenses(&doc, &uri);
    assert_eq!(lenses.len(), 2);
    assert_eq!(lenses[0].range.start.line, 3);
    assert_eq!(lenses[1].range.start.line, 6);
    let cmd = lenses[0].command.as_ref().unwrap();
    assert_eq!(cmd.command, "telar.preview");
    assert_eq!(cmd.arguments.as_ref().unwrap()[1], serde_json::json!("A"));
}
