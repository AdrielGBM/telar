use super::*;
use std::str::FromStr;

fn diag(message: &str) -> Diagnostic {
    Diagnostic {
        message: message.to_string(),
        ..Default::default()
    }
}

#[test]
fn create_class_inserts_at_end_of_style() {
    let src = "[style]\nprimary: #ffffff\n\n@card\n    width: 240\n[view]\ncol @missing\n";
    let uri = Uri::from_str("file:///x.rsx").unwrap();
    let actions = code_actions(
        src,
        &uri,
        &[diag("Style class `@missing` is not defined in [style]")],
    );
    assert_eq!(actions.len(), 1);
    let CodeActionOrCommand::CodeAction(a) = &actions[0] else {
        panic!()
    };
    assert!(a.title.contains("@missing"), "title: {}", a.title);
    let edits = a.edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri].clone();
    assert!(
        edits[0].new_text.contains("@missing"),
        "the edit must declare the missing class: {}",
        edits[0].new_text
    );
    assert_eq!(edits[0].range.start.line, 4);
}

#[test]
fn no_action_for_unrelated_diagnostics() {
    let src = "[view]\ncol\n";
    let uri = Uri::from_str("file:///x.rsx").unwrap();
    assert!(
        code_actions(src, &uri, &[diag("some other error")]).is_empty(),
        "an unrelated diagnostic offers no action"
    );
}
