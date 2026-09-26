use geometry_core::Rect;

use super::*;

fn node(role: Role, name: &str) -> AccessNode {
    AccessNode {
        id: None,
        role,
        name: name.to_string(),
        rect: Rect::default(),
        focused: false,
        enabled: true,
        toggled: None,
        value: None,
        lang: None,
    }
}

/// Text reads as itself; anything else says what it is after its name, and a state it carries after that.
#[test]
fn a_transcript_reads_each_node_as_a_line() {
    let mut agree = node(Role::CheckBox, "I agree");
    agree.toggled = Some(false);
    let mut send = node(Role::Button, "Send");
    send.enabled = false;
    let nodes = [
        node(Role::Label, "Terms"),
        node(Role::Drawing, "Company logo"),
        agree,
        send,
    ];
    assert_eq!(
        transcript(&nodes),
        "Terms\nCompany logo, image\nI agree, checkbox, not checked\nSend, button, unavailable"
    );
}

#[test]
fn nothing_to_read_is_an_empty_transcript() {
    assert_eq!(transcript(&[]), "");
}
