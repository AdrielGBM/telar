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

/// A press handler that shows a hidden field through a signal and focuses it in the same event: the event's batch has not yet run the effect that shows it, so the request is judged once it has.
#[test]
fn a_field_shown_and_focused_in_one_event_gets_the_focus() {
    use layout_core::{AvailableSpace, LayoutStyle};
    use platform_core::{Event, PointerButton, PointerSource};
    use reactive_core::signal;
    use renderer_core::{Color, RectStyle, TextStyle};

    use crate::layout_item::{LayoutItem, box_item};
    use crate::{ComponentList, Container, Input, StyledContainer, style_follows};

    crate::context::reset_layout_runtime();
    clear();
    let shown = signal(false);
    let field = Input::new(signal(String::new()), LayoutStyle::new(), || {
        TextStyle::new(14.0, Color::BLACK)
    })
    .unwrap();
    let field_id = field.focus_id();
    style_follows(field.layout_node(), move || match shown.get() {
        true => LayoutStyle::new().width(100.0).height(20.0),
        false => LayoutStyle::new().display_none(),
    });
    let opener = StyledContainer::new(
        LayoutStyle::new().width(100.0).height(20.0),
        |_| RectStyle::default(),
        vec![],
    )
    .unwrap()
    .on_press(move || {
        shown.set(true);
        request(field_id);
    });
    let column = Container::new(
        LayoutStyle::new().flex_column().width(200.0).height(100.0),
        vec![box_item(opener), box_item(field)],
    )
    .unwrap();
    crate::context::compute_layout(
        column.layout_node(),
        AvailableSpace::Definite(200.0),
        AvailableSpace::Definite(100.0),
    )
    .unwrap();
    let mut tree = ComponentList::new(box_item(column));

    let at = |pressed: bool| {
        let (x, y, button, source) = (10.0, 10.0, PointerButton::Primary, PointerSource::Mouse);
        match pressed {
            true => Event::PointerPressed {
                x,
                y,
                button,
                source,
            },
            false => Event::PointerReleased {
                x,
                y,
                button,
                source,
            },
        }
    };
    tree.on_event(&at(true));
    tree.on_event(&at(false));
    assert!(
        is_focused(field_id),
        "the field was hidden when asked and shown by the end of the event"
    );
}

/// A request judged later still refuses a field that stayed hidden.
#[test]
fn a_field_still_hidden_after_the_event_is_not_focused() {
    use layout_core::LayoutStyle;
    use reactive_core::{batch, signal};
    use renderer_core::{Color, TextStyle};

    use crate::layout_item::LayoutItem;
    use crate::{Input, style_follows};

    crate::context::reset_layout_runtime();
    clear();
    let field = Input::new(signal(String::new()), LayoutStyle::new(), || {
        TextStyle::new(14.0, Color::BLACK)
    })
    .unwrap();
    let field_id = field.focus_id();
    style_follows(field.layout_node(), || LayoutStyle::new().display_none());
    batch(|| request(field_id));
    assert!(!is_focused(field_id));
}

fn field_shown_by(shown: reactive_core::RwSignal<bool>) -> crate::Input {
    use layout_core::LayoutStyle;
    use renderer_core::{Color, TextStyle};

    use crate::layout_item::LayoutItem;

    let field = crate::Input::new(
        reactive_core::signal(String::new()),
        LayoutStyle::new(),
        || TextStyle::new(14.0, Color::BLACK),
    )
    .unwrap();
    crate::style_follows(field.layout_node(), move || match shown.get() {
        true => LayoutStyle::new().width(100.0).height(20.0),
        false => LayoutStyle::new().display_none(),
    });
    field
}

/// A hidden field asked for first and shown by the end of the batch must not take focus from a field asked for after it.
#[test]
fn a_deferred_request_loses_to_a_later_one() {
    use reactive_core::{batch, signal};

    crate::context::reset_layout_runtime();
    clear();
    let shown = signal(false);
    let hidden = field_shown_by(shown);
    let visible = field_shown_by(signal(true));

    batch(|| {
        request(hidden.focus_id());
        shown.set(true);
        request(visible.focus_id());
    });

    assert!(is_focused(visible.focus_id()));
}

/// A blur after a request that is still waiting on its batch is the later word.
#[test]
fn a_deferred_request_loses_to_a_later_release_or_clear() {
    use reactive_core::{batch, signal};

    crate::context::reset_layout_runtime();
    clear();
    let shown = signal(false);
    let field = field_shown_by(shown);

    batch(|| {
        request(field.focus_id());
        shown.set(true);
        release(field.focus_id());
    });
    assert_eq!(current(), None, "released");

    shown.set(false);
    batch(|| {
        request(field.focus_id());
        shown.set(true);
        clear();
    });
    assert_eq!(current(), None, "cleared");

    shown.set(false);
    batch(|| {
        request(field.focus_id());
        shown.set(true);
    });
    assert!(
        is_focused(field.focus_id()),
        "and one nothing overrode still lands"
    );
}

/// A request waiting on its batch names an id minted by its own surface; once that surface is gone the same number names a stranger in whichever world is active.
#[test]
fn a_deferred_request_from_a_surface_that_is_gone_does_nothing() {
    use reactive_core::{batch, signal};

    crate::context::reset_layout_runtime();
    clear();
    let bystander = field_shown_by(signal(true));
    let wanted_shown = signal(false);
    let wanted = field_shown_by(wanted_shown);

    batch(|| {
        let surface = crate::Surface::new();
        {
            let _entered = surface.enter();
            let shown = signal(false);
            let field = field_shown_by(shown);
            assert_eq!(
                field.focus_id(),
                bystander.focus_id(),
                "the two surfaces mint the same id"
            );
            request(field.focus_id());
            shown.set(true);
            drop(field);
        }
        drop(surface);
        request(wanted.focus_id());
        wanted_shown.set(true);
    });

    assert!(is_focused(wanted.focus_id()));
}

fn laid_out_box() -> crate::StyledContainer {
    use crate::context::compute_layout;
    use crate::{LayoutItem, StyledContainer};
    use layout_core::{AvailableSpace, LayoutStyle};

    let control = StyledContainer::new(
        LayoutStyle::new().width(50.0).height(20.0),
        |_r| renderer_core::RectStyle::default(),
        vec![],
    )
    .unwrap();
    compute_layout(
        control.layout_node(),
        AvailableSpace::Definite(50.0),
        AvailableSpace::Definite(20.0),
    )
    .unwrap();
    control
}

#[test]
fn a_control_keeps_what_its_role_keeps_and_is_a_tab_stop() {
    use crate::LayoutItem;
    crate::context::reset_layout_runtime();
    let node = laid_out_box().layout_node();
    let id = next_id();
    register_with_role(id, FocusKind::Widget, node, Role::Slider);

    let focusable = focusable_of(id, Role::Slider, None);
    assert!(focusable.tab_stop);
    assert_eq!(focusable.consumes, Role::Slider.consumed_keys());
}

#[test]
fn a_declared_set_replaces_the_role_s() {
    use crate::LayoutItem;
    crate::context::reset_layout_runtime();
    let node = laid_out_box().layout_node();
    let id = next_id();
    register_with_role(id, FocusKind::Widget, node, Role::Button);

    let declared = ConsumedKeys::ARROW_DOWN | ConsumedKeys::ACTIVATION;
    assert_eq!(
        focusable_of(id, Role::Button, Some(declared)).consumes,
        declared
    );
}

#[test]
fn a_control_driven_by_another_keeps_its_keys_without_being_a_stop() {
    use crate::LayoutItem;
    crate::context::reset_layout_runtime();
    let node = laid_out_box().layout_node();
    let id = next_id();
    register_presented(id, node, Role::MenuItem);

    let focusable = focusable_of(id, Role::MenuItem, None);
    assert!(!focusable.tab_stop);
    assert_eq!(focusable.consumes, Role::MenuItem.consumed_keys());
}

#[test]
fn an_id_nobody_registered_is_no_stop_and_keeps_nothing() {
    assert_eq!(
        focusable_of(next_id(), Role::Slider, None),
        Focusable::default()
    );
}

#[test]
fn inside_a_modal_that_holds_focus_tab_is_kept() {
    use crate::LayoutItem;
    crate::context::reset_layout_runtime();
    let node = laid_out_box().layout_node();
    let id = next_id();
    register_with_role(id, FocusKind::Widget, node, Role::Button);
    let open = reactive_core::signal(true);
    let scope = register_scope(node, move || open.get(), true);

    assert!(
        focusable_of(id, Role::Button, None)
            .consumes
            .contains(ConsumedKeys::TAB),
        "stepping inside the trap is Telar's"
    );
    open.set(false);
    let closed = focusable_of(id, Role::Button, None);
    assert!(!closed.consumes.contains(ConsumedKeys::TAB));
    unregister_scope(scope);
}

#[test]
fn a_host_focus_move_becomes_telar_s() {
    use crate::LayoutItem;
    crate::context::reset_layout_runtime();
    let node = laid_out_box().layout_node();
    let id = next_id();
    register_at(id, FocusKind::Widget, node);
    clear();

    assert!(follow_box(u64::from(node)));
    assert_eq!(current(), Some(id));
    assert!(is_focus_visible(id), "a Tab the host walked shows the ring");
    assert!(
        !follow_box(u64::from(node)),
        "the move reported for focus already there changes nothing"
    );
}

#[test]
fn a_host_focus_move_echoing_a_tap_keeps_the_ring_off() {
    use crate::LayoutItem;
    crate::context::reset_layout_runtime();
    let node = laid_out_box().layout_node();
    let id = next_id();
    register_at(id, FocusKind::Widget, node);

    request_from_pointer(id);
    follow_box(u64::from(node));
    assert!(is_focused(id));
    assert!(!is_focus_visible(id));
}

#[test]
fn a_host_focus_move_to_no_known_box_changes_nothing() {
    let id = next_id();
    register_as(id, FocusKind::Widget);
    request(id);

    assert!(!follow_box(u64::MAX));
    assert_eq!(current(), Some(id));
}
