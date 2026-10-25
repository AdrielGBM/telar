use std::cell::{Cell, RefCell};

use geometry_core::{Rect, Size};
use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{ConsumedKeys, Event, Role};
use ui_tree::{Component, ComponentList, EventResult};

use super::*;
use crate::accessibility::{AccessTarget, NestedSurface, perform_action, snapshot};
use crate::container::Container;
use crate::context::{compute_layout, reset_layout_runtime};
use crate::layout_item::LayoutItem;
use crate::{Surface, composite_surface, focus};

const AT: (f32, f32) = (30.0, 50.0);

/// A foreign surface with [`CONTROLS`] focusables in a row, which reports what it is told to and records what it is asked to do.
#[derive(Default)]
struct Fake {
    nodes: Vec<AccessNode>,
    actions: RefCell<Vec<(u64, bool)>>,
    input: Cell<Option<TextInput>>,
    focused: Cell<Option<u32>>,
}

const CONTROLS: u32 = 2;

impl ForeignSurface for Fake {
    fn access_snapshot(&self) -> Vec<AccessNode> {
        self.nodes.clone()
    }

    fn perform_access_action(&self, id: u64, activate: bool) {
        self.actions.borrow_mut().push((id, activate));
        self.focused.set(Some(0));
    }

    fn text_input(&self) -> Option<TextInput> {
        self.input.get()
    }

    fn arrive(&self, backwards: bool) -> bool {
        if self.focused.get().is_none() {
            self.focused
                .set(Some(if backwards { CONTROLS - 1 } else { 0 }));
        }
        true
    }

    fn tab(&self, key: &Event) -> bool {
        let backwards = matches!(key, Event::KeyPressed { modifiers, .. } if modifiers.is_shift);
        let at = self.focused.get().unwrap_or(0) as i64 + if backwards { -1 } else { 1 };
        let inside = (0..CONTROLS as i64).contains(&at);
        self.focused.set(inside.then_some(at as u32));
        !inside
    }

    fn blur(&self) {
        self.focused.set(None);
    }

    fn holds_focus(&self) -> bool {
        self.focused.get().is_some()
    }

    fn text_entry_focused(&self) -> bool {
        self.focused.get() == Some(0)
    }

    fn keeps(&self, keys: ConsumedKeys) -> bool {
        self.text_entry_focused() && ConsumedKeys::ENTER.contains(keys)
    }
}

fn button(id: u64, name: &str) -> AccessNode {
    AccessNode {
        id: Some(id),
        parent: None,
        role: Role::Button,
        name: name.to_string(),
        rect: Rect::new(0.0, 0.0, 20.0, 10.0),
        focused: false,
        enabled: true,
        toggled: None,
        value: None,
        orientation: None,
        expanded: None,
        position: None,
        lang: None,
        url: None,
        current: None,
        active_descendant: None,
        controls: None,
        has_popup: None,
        keyboard_shortcut: None,
        sort: None,
        multiselectable: false,
        modal: false,
    }
}

fn foreign(nodes: Vec<AccessNode>) -> (Rc<Fake>, ForeignBoundary) {
    let fake = Rc::new(Fake {
        nodes,
        ..Fake::default()
    });
    let source: Rc<dyn ForeignSurface> = fake.clone();
    let boundary = ForeignBoundary::new(&source);
    (fake, boundary)
}

/// The host's frame with `guest_frame` drawn at [`AT`] as the foreign surface's.
fn host_frame(boundary: &ForeignBoundary, guest_frame: Vec<DrawCommand>) -> Vec<DrawCommand> {
    struct Host(RefCell<Option<RenderNode>>);
    impl Component for Host {
        fn view(&self) -> RenderNode {
            self.0.borrow_mut().take().unwrap_or(RenderNode::Empty)
        }
    }
    let drawn = composite_surface(
        Transform::translate(AT.0, AT.1),
        Size::new(100.0, 100.0),
        [boundary.boundary(guest_frame)],
    );
    ComponentList::new(Host(RefCell::new(Some(drawn))))
        .commands()
        .clone()
}

fn read(commands: &[DrawCommand], name: &str) -> Option<AccessNode> {
    snapshot(commands)
        .into_iter()
        .find(|node| node.name == name)
}

fn host_world() {
    reset_layout_runtime();
    focus::clear();
}

#[test]
fn a_foreign_surface_is_read_as_it_reports_itself_where_the_frame_shows_it() {
    host_world();
    let (_fake, boundary) = foreign(vec![button(3, "Inside")]);
    let frame = host_frame(&boundary, Vec::new());

    let inside = read(&frame, "Inside").expect("the foreign surface's control is read");
    assert_eq!(inside.rect, Rect::new(AT.0, AT.1, 20.0, 10.0));
    assert_eq!(
        AccessTarget::of(inside.id.expect("an id")),
        AccessTarget::Nested {
            surface: boundary.key(),
            id: 3
        },
        "its id is qualified with the boundary's key"
    );
}

#[test]
fn a_readers_action_on_a_foreign_control_reaches_the_foreign_surface() {
    host_world();
    let (fake, boundary) = foreign(vec![button(3, "Inside")]);
    let frame = host_frame(&boundary, Vec::new());
    let id = read(&frame, "Inside").and_then(|node| node.id).unwrap();

    perform_action(id, true, |event| {
        panic!("nothing travels through the host's tree: {event:?}")
    });
    assert_eq!(*fake.actions.borrow(), [(3, true)]);
}

#[test]
fn a_foreign_key_is_never_a_surface_handle() {
    let before = Surface::new();
    let (_fake, boundary) = foreign(Vec::new());
    let after = Surface::new();
    let handles = [before.handle().raw(), after.handle().raw()];

    assert!(!handles.contains(&boundary.key()));
    assert!(handles[0] < boundary.key() && boundary.key() < handles[1]);
    assert!(
        boundary.key() >> NESTED_ID_SHIFT == 0,
        "the key fits where a qualified id carries it"
    );
}

#[test]
fn a_dropped_boundary_reports_nothing_and_takes_no_action() {
    host_world();
    let (fake, boundary) = foreign(vec![button(3, "Inside")]);
    let frame = host_frame(&boundary, Vec::new());
    let id = read(&frame, "Inside").and_then(|node| node.id).unwrap();
    let key = boundary.key();
    drop(boundary);

    let nested = NestedSurface {
        surface: key,
        commands: 0..0,
        placement: Transform::IDENTITY.to_array(),
    };
    assert!(nested.foreign().is_none());
    assert!(
        read(&frame, "Inside").is_none(),
        "a frame still marked with its key reads nothing of it"
    );
    let mut dispatched = Vec::new();
    perform_action(id, true, |event| dispatched.push(event.clone()));
    assert!(fake.actions.borrow().is_empty());
    assert!(matches!(
        dispatched.as_slice(),
        [Event::InSurface { surface, .. }] if *surface == key
    ));
}

/// A surface the foreign world nests in its own frame carries a handle of that world, which here may name a surface of this one.
#[test]
fn a_surface_nested_in_a_foreign_one_is_marked_and_addressed_under_a_key_of_its_own() {
    host_world();
    let guest = 1;
    let nested_id = (guest << NESTED_ID_SHIFT) | 4;
    let (fake, boundary) = foreign(vec![button(nested_id, "Deeper")]);
    let frame = host_frame(
        &boundary,
        vec![
            DrawCommand::PushSurface { surface: guest },
            DrawCommand::PopSurface,
        ],
    );

    let marked: Vec<u64> = frame
        .iter()
        .filter_map(|command| match command {
            DrawCommand::PushSurface { surface } => Some(*surface),
            _ => None,
        })
        .collect();
    let [outer, inner] = marked[..] else {
        panic!("two markers: {marked:?}");
    };
    assert_eq!(outer, boundary.key());
    assert!(inner != guest && inner != outer);

    let id = read(&frame, "Deeper").and_then(|node| node.id).unwrap();
    assert_eq!(
        AccessTarget::of(id),
        AccessTarget::Nested {
            surface: inner,
            id: 4
        }
    );
    perform_action(id, false, |event| {
        panic!("handed to the foreign surface: {event:?}")
    });
    assert_eq!(*fake.actions.borrow(), [(nested_id, false)]);

    let frame = host_frame(&boundary, Vec::new());
    assert_eq!(
        read(&frame, "Deeper").map(|node| node.id),
        Some(None),
        "once its frames no longer draw that surface, its id names nothing"
    );
}

#[test]
fn the_focus_stop_tells_the_input_method_where_the_foreign_caret_is() {
    host_world();
    let (fake, boundary) = foreign(Vec::new());
    let drawn_in = Container::new(LayoutStyle::new().width(100.0).height(40.0), vec![]).unwrap();
    let node = drawn_in.layout_node();
    let host = Container::new(
        LayoutStyle::new()
            .width(400.0)
            .height(400.0)
            .padding_all(20.0),
        vec![Box::new(drawn_in)],
    )
    .unwrap();
    compute_layout(
        host.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    let stop = boundary.focus_stop(node, || Transform::scale_around(2.0, 2.0, 0.0, 0.0));
    host.view();
    fake.input.set(Some(TextInput {
        field: 7,
        caret: Rect::new(5.0, 4.0, 1.0, 10.0),
        secret: false,
    }));

    assert_eq!(
        crate::text_input(),
        None,
        "the stop does not hold focus yet"
    );
    focus::request(stop.id());
    let input = crate::text_input().expect("the caret of the field inside");
    assert_eq!(input.field, 7u64.rotate_left(32) ^ stop.id());
    assert_eq!(input.caret, Rect::new(30.0, 28.0, 2.0, 20.0));
}

fn tab_key(backwards: bool) -> Event {
    Event::KeyPressed {
        key: platform_core::Key::Named(platform_core::NamedKey::Tab),
        modifiers: platform_core::ModifiersState {
            is_shift: backwards,
            ..platform_core::ModifiersState::default()
        },
        unmodified: None,
    }
}

/// A host row laid out and drawn: a focusable before the box the foreign surface is drawn in and one after it, by their focus ids, and that box's node.
fn host_row() -> (Container, focus::FocusId, NodeId, focus::FocusId) {
    let cell =
        |width: f32| Container::new(LayoutStyle::new().width(width).height(20.0), vec![]).unwrap();
    let (before, drawn_in, after) = (cell(20.0), cell(100.0), cell(20.0));
    let nodes = [
        before.layout_node(),
        drawn_in.layout_node(),
        after.layout_node(),
    ];
    let row = Container::new(
        LayoutStyle::new().flex_row().width(400.0).height(400.0),
        vec![Box::new(before), Box::new(drawn_in), Box::new(after)],
    )
    .unwrap();
    compute_layout(
        row.layout_node(),
        AvailableSpace::Definite(400.0),
        AvailableSpace::Definite(400.0),
    )
    .unwrap();
    row.view();
    let stop = |node| {
        let id = focus::next_id();
        focus::register_at(id, focus::FocusKind::Widget, node);
        id
    };
    let before = stop(nodes[0]);
    (row, before, nodes[1], stop(nodes[2]))
}

/// One focus owner throughout: the keyboard walks into the foreign surface through its stop, through its controls, and out the other side, and focus leaving the stop leaves nothing focused inside.
#[test]
fn tab_walks_into_the_foreign_surface_through_its_stop_and_out_again() {
    host_world();
    let (fake, boundary) = foreign(Vec::new());
    let (_row, before, node, after) = host_row();
    let stop = boundary.focus_stop(node, || Transform::IDENTITY);

    focus::request(before);
    focus::focus_next();
    assert!(focus::is_focused(stop.id()));
    assert_eq!(
        fake.focused.get(),
        Some(0),
        "Tab hands it the first control"
    );

    stop.tab(&tab_key(false));
    assert!(focus::is_focused(stop.id()), "Tab inside stays inside");
    assert_eq!(fake.focused.get(), Some(1));

    stop.tab(&tab_key(false));
    assert_eq!(
        focus::current(),
        Some(after),
        "past its last, on to the next stop"
    );
    assert_eq!(fake.focused.get(), None);

    focus::focus_prev();
    assert!(focus::is_focused(stop.id()));
    assert_eq!(fake.focused.get(), Some(1), "Shift+Tab hands it the last");

    stop.tab(&tab_key(true));
    stop.tab(&tab_key(true));
    assert_eq!(focus::current(), Some(before));
    assert_eq!(fake.focused.get(), None);

    focus::focus_next();
    focus::request(after);
    assert_eq!(
        fake.focused.get(),
        None,
        "losing the stop clears focus inside"
    );
}

#[test]
fn focus_taken_inside_brings_the_stop_focus_and_a_dropped_stop_clears_it() {
    host_world();
    let (fake, boundary) = foreign(Vec::new());
    let (_row, before, node, _after) = host_row();
    let stop = boundary.focus_stop(node, || Transform::IDENTITY);
    focus::request(before);

    fake.focused.set(Some(1));
    stop.follow_inside();
    assert!(focus::is_focused(stop.id()), "the keys follow focus inside");

    let id = stop.id();
    drop(stop);
    assert_eq!(fake.focused.get(), None);
    assert_eq!(focus::current(), None);
    assert!(focus::exposed().iter().all(|exposed| exposed.id != id));
}

#[test]
fn a_box_event_reaches_the_foreign_world_by_the_name_it_knows_its_surface_by() {
    host_world();
    let guest = 1;
    let (_fake, boundary) = foreign(Vec::new());
    let frame = host_frame(
        &boundary,
        vec![
            DrawCommand::PushSurface { surface: guest },
            DrawCommand::PopSurface,
        ],
    );
    let nested = frame
        .iter()
        .filter_map(|command| match command {
            DrawCommand::PushSurface { surface } => Some(*surface),
            _ => None,
        })
        .nth(1)
        .expect("the nested surface is marked");
    let focused = Event::BoxFocused { box_id: 9 };
    let inside = |surface| Event::InSurface {
        surface,
        event: Box::new(focused.clone()),
    };

    assert_eq!(
        boundary.foreign_event(&inside(boundary.key())),
        Some(focused.clone())
    );
    assert_eq!(boundary.foreign_event(&inside(nested)), Some(inside(guest)));
    assert_eq!(
        boundary.foreign_event(&inside(Surface::new().handle().raw())),
        None
    );
    assert_eq!(boundary.foreign_event(&focused), None);
}

#[test]
fn a_frame_the_foreign_surface_fills_is_marked_whole() {
    host_world();
    let (_fake, boundary) = foreign(Vec::new());
    let marked = boundary.marked([DrawCommand::PopClip]);
    assert_eq!(
        marked,
        [
            DrawCommand::PushSurface {
                surface: boundary.key()
            },
            DrawCommand::PopClip,
            DrawCommand::PopSurface
        ]
    );
}

#[test]
fn a_readers_move_into_the_foreign_surface_brings_its_stop_the_keys() {
    host_world();
    let (fake, boundary) = foreign(vec![button(3, "Inside")]);
    let (_row, before, node, _after) = host_row();
    let stop = boundary.focus_stop(node, || Transform::IDENTITY);
    focus::request(before);
    let frame = host_frame(&boundary, Vec::new());
    let id = read(&frame, "Inside").and_then(|node| node.id).unwrap();

    perform_action(id, false, |event| {
        panic!("handed to the foreign surface: {event:?}")
    });
    assert!(fake.holds_focus());
    assert!(
        focus::is_focused(stop.id()),
        "the keys that follow reach the control the reader moved to"
    );
}

fn named(key: platform_core::NamedKey) -> Event {
    Event::KeyPressed {
        key: platform_core::Key::Named(key),
        modifiers: platform_core::ModifiersState::default(),
        unmodified: None,
    }
}

/// A host dialog around a foreign form: what the dismiss stack asks of the focused control is answered by the control focused inside the foreign surface, as it is for a nested canvas.
#[test]
fn enter_and_escape_in_a_foreign_field_neither_confirm_nor_dismiss_the_hosts_dialog() {
    use platform_core::NamedKey;
    host_world();
    let (fake, boundary) = foreign(Vec::new());
    let (_row, _before, node, _after) = host_row();
    let stop = boundary.focus_stop(node, || Transform::IDENTITY);
    let decided = Rc::new(RefCell::new(Vec::new()));
    let (dismissed, confirmed) = (Rc::clone(&decided), Rc::clone(&decided));
    let _dialog = crate::DismissRegistration::confirmable(
        Rc::new(move || dismissed.borrow_mut().push("dismissed")),
        Rc::new(move || confirmed.borrow_mut().push("confirmed")),
    );
    focus::request(stop.id());
    fake.focused.set(Some(0));

    assert_eq!(
        crate::dispatch_overlays(&named(NamedKey::Enter)),
        EventResult::Ignored
    );
    assert_eq!(
        crate::dispatch_overlays(&named(NamedKey::Escape)),
        EventResult::Ignored
    );
    assert!(
        decided.borrow().is_empty(),
        "the field keeps them: {decided:?}"
    );
    assert!(
        focus::text_entry_focused(),
        "a shortcut table hears somebody is typing"
    );

    fake.focused.set(Some(1));
    assert_eq!(
        crate::dispatch_overlays(&named(NamedKey::Enter)),
        EventResult::Handled
    );
    assert_eq!(
        *decided.borrow(),
        ["confirmed"],
        "a control with no use for Enter lets the dialog have it"
    );
}

/// The stop outlives the scope that registered it, and so does what follows its focus.
#[test]
fn a_stop_keeps_following_its_focus_once_the_scope_that_registered_it_is_gone() {
    host_world();
    let (fake, boundary) = foreign(Vec::new());
    let (_row, before, node, _after) = host_row();
    let scope = reactive_core::owner_scope();
    let stop = boundary.focus_stop(node, || Transform::IDENTITY);
    let registered_in = scope.id();
    drop(scope);
    reactive_core::dispose_owner(registered_in);

    focus::request(before);
    focus::focus_next();
    assert_eq!(
        fake.focused.get(),
        Some(0),
        "the keyboard still arrives inside"
    );
    focus::request(before);
    assert_eq!(fake.focused.get(), None, "and leaving still blurs it");
    stop.follow_inside();
    assert_eq!(
        focus::current(),
        Some(before),
        "so it never takes focus back"
    );
}

#[test]
fn a_readers_move_after_the_stop_is_dropped_requests_no_ghost_stop() {
    host_world();
    let (fake, boundary) = foreign(vec![button(3, "Inside")]);
    let (_row, before, node, _after) = host_row();
    let stop = boundary.focus_stop(node, || Transform::IDENTITY);
    let frame = host_frame(&boundary, Vec::new());
    let id = read(&frame, "Inside").and_then(|node| node.id).unwrap();
    drop(stop);
    focus::request(before);

    perform_action(id, false, |event| {
        panic!("handed to the foreign surface: {event:?}")
    });
    assert!(fake.holds_focus());
    assert_eq!(focus::current(), Some(before));
}

/// The stop answers in the world it was registered in, whichever world is entered when it is asked.
#[test]
fn a_stop_follows_focus_inside_in_its_own_world() {
    host_world();
    let (fake, boundary) = foreign(Vec::new());
    let (_row, _before, node, _after) = host_row();
    let stop = boundary.focus_stop(node, || Transform::IDENTITY);
    fake.focused.set(Some(1));

    Surface::new().with_entered(|| stop.follow_inside());
    assert!(focus::is_focused(stop.id()));
    assert!(stop.holds_keys());
}
