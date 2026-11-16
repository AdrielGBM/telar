use geometry_core::{Rect, Size};
use platform_accesskit::accesskit::{ActionData, NodeId, Role as AkRole, ScrollUnit, TreeId};
use platform_accesskit::{WindowFrame, requested_action, tree_update};
use platform_core::{
    AccessAction, AccessId, AccessNode, ActionSupport, FocusId, NumericValue, Role, Value,
};

use super::*;

fn node(id: Option<u64>, role: Role, name: &str) -> AccessNode {
    AccessNode {
        id: id.map(|id| AccessId::own(FocusId(id))),
        parent: None,
        role,
        name: name.to_string(),
        rect: Rect::new(0.0, 0.0, 10.0, 10.0),
        focused: false,
        enabled: true,
        toggled: None,
        value: None,
        description: None,
        invalid: false,
        required: false,
        readonly: false,
        lang: None,
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
        selection: None,
        line_starts: Vec::new(),
        supports: ActionSupport::default(),
    }
}

fn volume() -> AccessNode {
    AccessNode {
        value: Some(Value::from(
            NumericValue::new(0.0..=10.0, 4.0).expect("a reading within its range"),
        )),
        supports: ActionSupport {
            step: true,
            ..ActionSupport::default()
        },
        ..node(Some(9), Role::Slider, "Volume")
    }
}

fn published(nodes: &[AccessNode]) -> TreeUpdate {
    let mut update = tree_update(
        nodes,
        WindowFrame {
            title: "Mixer",
            size: Size::new(400.0, 300.0),
            scale_factor: 1.0,
        },
        None,
    );
    offer_steps_as_scrolls(&mut update);
    update
}

fn slider_of(update: &TreeUpdate) -> NodeId {
    update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == AkRole::Slider)
        .expect("the slider is published")
        .0
}

fn scroll(target: NodeId, action: Action) -> ActionRequest {
    ActionRequest {
        action,
        target_tree: TreeId::ROOT,
        target_node: target,
        data: Some(ActionData::ScrollUnit(ScrollUnit::Page)),
    }
}

/// A reader on Android offers its adjust gesture only on a node that says it scrolls, so a slider that steps says so, and a slider that does not step, or a label, does not.
#[test]
fn a_slider_that_steps_is_offered_to_the_reader_as_adjustable() {
    let fixed = node(Some(10), Role::Slider, "Balance");
    let caption = node(None, Role::Label, "Mixer");
    let update = published(&[volume(), fixed, caption]);

    let offered = |name: &str| {
        let node = &update
            .nodes
            .iter()
            .find(|(_, node)| node.label() == Some(name))
            .expect("the node is published")
            .1;
        node.supports_action(Action::ScrollUp) && node.supports_action(Action::ScrollDown)
    };
    assert!(offered("Volume"));
    assert!(!offered("Balance"));
    assert!(!offered("Mixer"));
}

/// Swiping a slider forward raises it a step and swiping it back lowers it, whichever axis the adapter reads the swipe along.
#[test]
fn a_scroll_on_a_slider_reaches_it_as_its_step() {
    let nodes = [volume()];
    let slider = slider_of(&published(&nodes));
    let stepped = |action| requested_action(&as_step(scroll(slider, action)), &nodes);
    let volume = AccessId::own(FocusId(9));

    for forward in [Action::ScrollDown, Action::ScrollRight] {
        assert_eq!(
            stepped(forward),
            Some((volume, AccessAction::Increment)),
            "{forward:?}"
        );
    }
    for backward in [Action::ScrollUp, Action::ScrollLeft] {
        assert_eq!(
            stepped(backward),
            Some((volume, AccessAction::Decrement)),
            "{backward:?}"
        );
    }
}

/// Every other request is the one the reader made: a double tap still presses.
#[test]
fn a_request_that_is_no_scroll_passes_unchanged() {
    let nodes = [volume()];
    let slider = slider_of(&published(&nodes));
    let click = ActionRequest {
        action: Action::Click,
        target_tree: TreeId::ROOT,
        target_node: slider,
        data: None,
    };
    assert_eq!(
        requested_action(&as_step(click), &nodes),
        Some((AccessId::own(FocusId(9)), AccessAction::Activate))
    );
}
