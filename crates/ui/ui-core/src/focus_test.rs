use super::*;

#[test]
fn request_release_and_ids_are_unique() {
    clear();
    let a = next_id();
    let b = next_id();
    assert_ne!(a, b, "ids must be unique");

    assert!(!is_focused(a), "a fresh id holds no focus");
    request(a);
    assert!(
        is_focused(a) && current() == Some(a),
        "requesting focus both marks the id and makes it current"
    );

    request(b);
    assert!(
        is_focused(b) && !is_focused(a),
        "focus moves rather than being shared"
    );

    release(a);
    assert!(
        is_focused(b),
        "releasing an id that never had focus changes nothing"
    );

    release(b);
    assert!(
        current().is_none(),
        "releasing the focused id leaves nobody focused"
    );
}

/// A control the application has disabled is not a Tab stop — in HTML a `disabled` element is skipped outright, and a keyboard user made to walk through controls that do nothing is being told less about the interface than a mouse user, who at least sees them dimmed.
///
/// Rides the same scope mechanism a hidden overlay uses rather than a second one, which is what gives a disabled *wrapper* the `fieldset` reading for the keyboard as well as for the pointer.
#[test]
fn tab_skips_a_disabled_box() {
    use crate::context::{compute_layout, reset_layout_runtime};
    use crate::{LayoutItem, StyledContainer};
    use layout_core::{AvailableSpace, LayoutStyle};

    reset_layout_runtime();
    let base = next_id();
    register_as(base, FocusKind::Widget);

    let below = next_id();
    let off = StyledContainer::new(
        LayoutStyle::new().width(50.0).height(20.0),
        |_r| renderer_core::RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_focus(|_| {})
    .disabled(|| true);
    let above = next_id();
    compute_layout(
        off.layout_node(),
        AvailableSpace::Definite(50.0),
        AvailableSpace::Definite(20.0),
    )
    .unwrap();

    request(base);
    focus_next();
    let landed = current().expect("something took focus");
    assert!(
        !(landed > below && landed < above),
        "Tab landed on a box the application had disabled"
    );
}

/// The case that showed an overlay-shaped fix would only ever be half of one: `display:none` hides content without any overlay involved, and left it in the tab order just the same. The two mechanisms leave opposite traces — a hidden overlay keeps its children's rects and stops painting, a `display:none` subtree collapses to zero and keeps its place in the walk — so neither a paint test nor a rect test catches both. Ancestry does.
#[test]
fn tab_skips_a_focusable_taken_out_of_layout_flow() {
    use crate::context::{compute_layout, reset_layout_runtime, set_display};
    use crate::{LayoutItem, StyledContainer};
    use layout_core::{AvailableSpace, LayoutStyle};

    reset_layout_runtime();
    let base = next_id();
    register_as(base, FocusKind::Widget);

    let below = next_id();
    let hidden = StyledContainer::new(
        LayoutStyle::new().width(50.0).height(20.0),
        |_r| renderer_core::RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_focus(|_| {});
    let node = hidden.layout_node();
    let root = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(100.0),
        |_r| renderer_core::RectStyle::default(),
        vec![Box::new(hidden)],
    )
    .unwrap();
    let above = next_id();

    set_display(node, false);
    compute_layout(
        root.layout_node(),
        AvailableSpace::Definite(100.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();

    request(base);
    focus_next();
    let landed = current().expect("something took focus");
    assert!(
        !(landed > below && landed < above),
        "Tab landed on a focusable that is out of layout flow"
    );
}

#[test]
fn tab_order_steps_forward_and_back() {
    let (a, b, c) = (next_id(), next_id(), next_id());
    register_as(a, FocusKind::Widget);
    register_as(b, FocusKind::Widget);
    register_as(c, FocusKind::Widget);

    request(a);
    focus_next();
    assert_eq!(current(), Some(b));
    focus_next();
    assert_eq!(current(), Some(c));
    focus_prev();
    assert_eq!(current(), Some(b));

    unregister(b);
    assert!(current().is_none(), "the walk starts with nothing focused");
    unregister(a);
    unregister(c);
}
