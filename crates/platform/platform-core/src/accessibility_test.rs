use geometry_core::Rect;

use super::*;

fn node(role: Role, name: &str) -> AccessNode {
    AccessNode {
        id: None,
        parent: None,
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
        controls: None,
        has_popup: None,
        keyboard_shortcut: None,
        sort: None,
        multiselectable: false,
        modal: false,
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
        toggled(Role::MenuItemCheckBox, "Word wrap", true),
        toggled(Role::MenuItemRadio, "Compact", false),
    ];
    assert_eq!(
        transcript(&nodes),
        "Reduce motion, switch, checked\nBold, button, pressed\nItalic, button, not pressed\nGeneral, tab, selected\nAdvanced, tab\nDetails, button, collapsed\nVolume, slider\nWord wrap, menuitemcheckbox, checked\nCompact, menuitemradio, not checked"
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

/// A control that opens something says what before whether it is open, as a reader warns of a menu before Enter drops it.
#[test]
fn a_control_that_opens_something_says_what() {
    let opening = |role: Role, name: &str, popup: HasPopup, open: bool| {
        let mut node = node(role, name);
        node.has_popup = Some(popup);
        node.expanded = Some(open);
        node
    };
    let nodes = [
        opening(Role::Button, "File", HasPopup::Menu, false),
        opening(Role::ComboBox, "Size", HasPopup::ListBox, true),
    ];
    assert_eq!(
        transcript(&nodes),
        "File, button, has menu, collapsed\nSize, combobox, has listbox, expanded"
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

/// A control with a key that runs its command says so after its role and what it opens.
#[test]
fn a_control_with_a_shortcut_says_it() {
    let mut undo = node(Role::MenuItem, "Undo");
    undo.keyboard_shortcut = Some("Control+Z".to_string());
    assert_eq!(
        transcript(&[undo, node(Role::MenuItem, "Close")]),
        "Undo, menuitem, shortcut Control+Z\nClose, menuitem"
    );
}

/// A list that lets several of its options be chosen says so after its role, and its options say which are chosen.
#[test]
fn a_list_of_several_choices_says_so() {
    let mut list = node(Role::ListBox, "Recipients");
    list.multiselectable = true;
    let mut ada = node(Role::Option, "Ada");
    ada.toggled = Some(true);
    let mut alan = node(Role::Option, "Alan");
    alan.toggled = Some(false);
    assert_eq!(
        transcript(&[list, ada, alan, node(Role::ListBox, "Sender")]),
        "Recipients, listbox, multiselectable\nAda, option, selected\nAlan, option\nSender, listbox"
    );
}

/// An item that says where it sits is read with the numbers its role takes, and a row with no name of its own, such as a grid's header row, reads as its role and its place.
#[test]
fn an_item_says_where_it_sits() {
    let place = |position, size| {
        Some(SetPosition {
            level: 2,
            position,
            size,
        })
    };
    let mut header = node(Role::Row, "");
    header.position = place(1, 501);
    let mut src = node(Role::TreeItem, "src");
    src.position = place(3, 4);
    let mut ada = node(Role::Option, "Ada");
    ada.position = place(1, 2);
    let mut save = node(Role::Button, "Save");
    save.position = place(1, 1);
    assert_eq!(
        transcript(&[header, src, ada, save]),
        "row, 1 of 501\nsrc, treeitem, level 2, 3 of 4\nAda, option, 1 of 2\nSave, button"
    );
}

/// A node is written under the node it sits in, two spaces further in for every level, so a flat reading keeps the nesting a reader steps through.
#[test]
fn a_transcript_indents_each_node_under_the_one_it_sits_in() {
    let inside = |parent: usize, node: AccessNode| AccessNode {
        parent: Some(parent),
        ..node
    };
    let nodes = [
        node(Role::Dialog, "Confirm"),
        inside(0, node(Role::Label, "Delete it?")),
        inside(0, node(Role::List, "")),
        inside(2, node(Role::ListItem, "notes.txt")),
        inside(0, node(Role::Button, "OK")),
        node(Role::Label, "After"),
    ];
    assert_eq!(
        transcript(&nodes),
        "Confirm, dialog\n  Delete it?\n  list\n    notes.txt, listitem\n  OK, button\nAfter"
    );
    assert_eq!(depths(&nodes), [0, 1, 1, 2, 1, 0]);
    assert_eq!(children_of(&nodes, Some(0)).collect::<Vec<_>>(), [1, 2, 4]);
    assert_eq!(children_of(&nodes, None).collect::<Vec<_>>(), [0, 5]);
}
