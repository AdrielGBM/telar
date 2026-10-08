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
        orientation: None,
        expanded: None,
        position: None,
        active_descendant: None,
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

/// A tree row says whether it is the chosen one and, apart from that, whether it is open; a leaf says neither.
#[test]
fn a_tree_row_is_read_as_selected_and_open_or_shut() {
    let row = |name: &str, selected: bool, expanded: Option<bool>| {
        let mut node = node(Role::TreeItem, name);
        node.toggled = Some(selected);
        node.expanded = expanded;
        node
    };
    let nodes = [
        row("Inputs", false, Some(true)),
        row("Button", true, Some(false)),
        row("Primary", false, None),
    ];
    assert_eq!(
        transcript(&nodes),
        "Inputs, treeitem, expanded\nButton, treeitem, selected, collapsed\nPrimary, treeitem"
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

/// A tree keeps focus while its cursor walks the rows, and a reader says the row the cursor is on rather than the tree around it.
#[test]
fn focus_is_read_on_the_row_a_focused_tree_s_cursor_rests_on() {
    let row = |id: u64, name: &str| {
        let mut node = node(Role::TreeItem, name);
        node.id = Some(id);
        node
    };
    let mut tree = node(Role::Tree, "Files");
    tree.id = Some(1);
    tree.focused = true;
    tree.active_descendant = Some(3);
    let nodes = [tree.clone(), row(2, "src"), row(3, "docs")];
    assert_eq!(
        transcript(&nodes),
        "Files, tree\nsrc, treeitem\ndocs, treeitem, focused"
    );

    tree.active_descendant = Some(9);
    assert_eq!(
        transcript(&[tree, row(2, "src")]),
        "Files, tree, focused\nsrc, treeitem",
        "a cursor on a row that is not built leaves focus on the tree"
    );
}
