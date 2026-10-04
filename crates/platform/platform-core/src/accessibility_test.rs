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
        url: None,
        current: None,
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

/// A reading with no ring to look at has to say where the keyboard is, or a terminal reader following it cannot tell which line Enter would press.
#[test]
fn the_focused_node_says_so_last() {
    let mut save = node(Role::Button, "Save");
    save.focused = true;
    assert_eq!(
        transcript(&[node(Role::Button, "Cancel"), save]),
        "Cancel, button\nSave, button, focused"
    );
}

/// A switch is checked, a toggle button pressed, a tab selected and a disclosure expanded: one flag, said the way its role says it.
#[test]
fn a_state_is_read_in_the_words_of_its_role() {
    let toggled = |role: Role, name: &str, on: bool| {
        let mut node = node(role, name);
        node.toggled = Some(on);
        node
    };
    let nodes = [
        toggled(Role::Switch, "Reduce motion", true),
        toggled(Role::Button, "Bold", true),
        toggled(Role::Button, "Italic", false),
        toggled(Role::Tab, "General", true),
        toggled(Role::Tab, "Advanced", false),
        toggled(Role::Disclosure, "Details", false),
        toggled(Role::Slider, "Volume", true),
    ];
    assert_eq!(
        transcript(&nodes),
        "Reduce motion, switch, checked\nBold, button, pressed\nItalic, button, not pressed\nGeneral, tab, selected\nAdvanced, tab\nDetails, button, collapsed\nVolume, slider"
    );
}

/// The link to where the reader is says so after its role, before whether it is focused, whatever it is the current one of.
#[test]
fn a_current_link_says_so() {
    let link = |name: &str, current: Option<CurrentKind>| {
        let mut node = node(Role::Link, name);
        node.current = current;
        node
    };
    let mut page = link("Home", Some(CurrentKind::Page));
    page.focused = true;
    let nodes = [
        page,
        link("Desktop", Some(CurrentKind::Location)),
        link("Simulation", None),
        link("EN", Some(CurrentKind::Item)),
    ];
    assert_eq!(
        transcript(&nodes),
        "Home, link, current, focused\nDesktop, link, current\nSimulation, link\nEN, link, current"
    );
}
