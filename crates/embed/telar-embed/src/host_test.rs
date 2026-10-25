use std::cell::RefCell;
use std::rc::Rc;

use geometry_core::Rect;
use layout_core::{AvailableSpace, LayoutStyle};
use platform_core::{AccessNode, Ime, Key, ModifiersState, NamedKey, PointerSource};
use reactive_core::{RwSignal, signal};
use renderer_core::{DrawCommand, RectStyle, TextStyle};
use ui_core::accessibility::{AccessTarget, perform_action, snapshot};
use ui_core::focus::{self, FocusId, FocusKind, Role};
use ui_core::{
    Container, EventResult, Input, LayoutItem, StyledContainer, compute_layout,
    reset_layout_runtime,
};
use ui_tree::{Component, ComponentList, RenderNode};

use super::*;
use crate::EmbeddedApp;

/// Where the host draws the guest, in a box of its own tree.
const AT: Rect = Rect {
    x: 40.0,
    y: 20.0,
    width: 200.0,
    height: 100.0,
};

/// A guest with a text field over a button, whose title says what the field holds and how often the button was pressed, and which panics on a secondary press.
#[derive(Default)]
struct Form {
    content: Option<Box<dyn LayoutItem>>,
    value: Option<RwSignal<String>>,
    presses: Option<RwSignal<u32>>,
}

impl EmbeddedApp for Form {
    fn build(&mut self) {
        renderer_core::set_default_text_metrics(renderer_text::ShaperMetrics);
        let value = signal(String::from("ab"));
        let presses = signal(0);
        let field = Input::new(value, LayoutStyle::new().width(100.0).height(20.0), || {
            TextStyle::new(14.0, Color::BLACK)
        })
        .expect("the field builds");
        let button = StyledContainer::new(
            LayoutStyle::new().width(60.0).height(24.0),
            |_| RectStyle::default(),
            vec![],
        )
        .expect("the button builds")
        .control(Role::Button)
        .on_press(move || presses.update(|presses| *presses += 1));
        let form = Container::new(
            LayoutStyle::new()
                .flex_column()
                .width(AT.width)
                .height(AT.height),
            vec![Box::new(field), Box::new(button)],
        )
        .expect("the form builds");
        self.content = Some(Box::new(form));
        self.value = Some(value);
        self.presses = Some(presses);
    }

    fn layout_root(&self) -> NodeId {
        self.content.as_ref().expect("built").layout_node()
    }

    fn view(&self) -> RenderNode {
        self.content.as_ref().expect("built").view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if matches!(
            event,
            Event::PointerPressed {
                button: PointerButton::Secondary,
                ..
            }
        ) {
            panic!("the form fell over");
        }
        self.content.as_mut().expect("built").on_event(event)
    }

    fn title(&self) -> String {
        let (value, presses) = (self.value.expect("built"), self.presses.expect("built"));
        format!("{} pressed {}", value.get(), presses.get())
    }

    fn id(&self) -> String {
        "form".into()
    }
}

unsafe extern "C" fn create_form(
    args: LentBytes,
    instance: *mut Handle<GuestInstance>,
) -> platform_core::guest::CallOutcome {
    unsafe { crate::guest::create(args, instance, |_| Box::new(Form::default())) }
}

/// A host whose tree has a focusable before the box the guest is drawn in at [`AT`] and one after it, laid out and drawn, with the guest's stop in that box.
struct Host {
    _row: Container,
    before: FocusId,
    stop: FocusId,
    after: FocusId,
    embed: LoadedEmbed,
}

impl Host {
    fn new() -> Self {
        reset_layout_runtime();
        focus::clear();
        let embed = LoadedEmbed::start(EmbedEntries::serving(create_form), &[], None)
            .expect("the guest loads");
        embed.relayout(AT.width, AT.height);
        let cell = |width: f32, height: f32| {
            Container::new(LayoutStyle::new().width(width).height(height), vec![]).unwrap()
        };
        let (before, drawn_in, after) = (
            cell(20.0, 20.0),
            cell(AT.width, AT.height),
            cell(20.0, 20.0),
        );
        let nodes = [
            before.layout_node(),
            drawn_in.layout_node(),
            after.layout_node(),
        ];
        let row = Container::new(
            LayoutStyle::new()
                .flex_row()
                .width(400.0)
                .height(400.0)
                .padding_all(20.0),
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
        let stop_at = |node| {
            let id = focus::next_id();
            focus::register_with_role(id, FocusKind::Widget, node, Role::Group);
            id
        };
        let before = stop_at(nodes[0]);
        let stop = embed.focus_stop(nodes[1], || Transform::IDENTITY);
        Self {
            _row: row,
            before,
            stop,
            after: stop_at(nodes[2]),
            embed,
        }
    }

    /// The host's frame, the guest's drawn at [`AT`].
    fn frame(&self) -> Vec<DrawCommand> {
        let drawn = crate::composite(AT, self.embed.boundary(), self.embed.paint());
        ComponentList::new(Drawn(RefCell::new(Some(drawn))))
            .commands()
            .clone()
    }

    /// Tab from the stop before the guest's.
    fn tab_in(&self) {
        focus::request(self.before);
        focus::focus_next();
    }
}

struct Drawn(RefCell<Option<RenderNode>>);

impl Component for Drawn {
    fn view(&self) -> RenderNode {
        self.0.borrow_mut().take().unwrap_or(RenderNode::Empty)
    }
}

fn read(nodes: &[AccessNode], role: Role) -> AccessNode {
    nodes
        .iter()
        .find(|node| node.role == role)
        .cloned()
        .unwrap_or_else(|| panic!("a {role:?} is read: {nodes:?}"))
}

fn tab(backwards: bool) -> Event {
    Event::KeyPressed {
        key: Key::Named(NamedKey::Tab),
        modifiers: ModifiersState {
            is_shift: backwards,
            ..ModifiersState::default()
        },
        unmodified: None,
    }
}

#[test]
fn the_guests_controls_are_read_where_the_host_draws_them_under_ids_of_their_own() {
    let host = Host::new();
    let nodes = snapshot(&host.frame());
    let key = host.embed.boundary().key();
    for role in [Role::TextInput, Role::Button] {
        let node = read(&nodes, role);
        assert!(
            matches!(
                AccessTarget::of(node.id.expect("a control")),
                AccessTarget::Nested { surface, .. } if surface == key
            ),
            "{role:?} is named as the guest's"
        );
    }
    let field = read(&nodes, Role::TextInput).rect;
    assert_eq!(
        (field.x, field.y),
        (AT.x, AT.y),
        "the field, at the guest's origin"
    );
    let button = read(&nodes, Role::Button).rect;
    assert_eq!(
        (button.x, button.y),
        (AT.x, AT.y + 20.0),
        "the button, under it"
    );
}

#[test]
fn a_readers_action_presses_the_guests_button_and_its_stop_takes_the_keys() {
    let host = Host::new();
    let button = read(&snapshot(&host.frame()), Role::Button)
        .id
        .expect("a control");

    perform_action(button, true, |event| {
        panic!("no tree of the host's carries it: {event:?}")
    });
    assert_eq!(host.embed.title(), "ab pressed 1");
    assert!(
        focus::is_focused(host.stop),
        "the keys that follow reach the button the reader is on"
    );
}

#[test]
fn the_guests_caret_is_placed_where_the_host_draws_it() {
    let host = Host::new();
    host.tab_in();
    host.frame();
    let input = ui_core::text_input().expect("the host's input method hears the guest's field");
    assert!(
        AT.contains(input.caret.x, input.caret.y) && input.caret.x > AT.x,
        "on the guest's field, after its text: {input:?}"
    );
}

/// One focus owner throughout: the keyboard walks into the guest through its stop, through its field and its button, and out the other side, and the stop holds focus exactly while something inside does.
#[test]
fn tab_walks_into_the_guest_through_its_stop_and_out_again() {
    let host = Host::new();
    let inside = host.embed.foreign_surface();
    let focused = |role| read(&inside.access_snapshot(), role).focused;
    let one_owner = || {
        assert_eq!(
            inside.holds_focus(),
            focus::is_focused(host.stop),
            "focus is inside exactly while the stop holds it"
        )
    };

    host.tab_in();
    one_owner();
    assert!(
        focused(Role::TextInput),
        "Tab hands the guest's first control focus"
    );

    assert!(host.embed.on_event(&tab(false)));
    one_owner();
    assert!(focused(Role::Button));

    assert!(host.embed.on_event(&tab(false)));
    one_owner();
    assert_eq!(
        focus::current(),
        Some(host.after),
        "past its last, on through the host"
    );

    focus::focus_prev();
    one_owner();
    assert!(focused(Role::Button), "Shift+Tab hands it its last");

    host.embed.on_event(&tab(true));
    host.embed.on_event(&tab(true));
    one_owner();
    assert_eq!(focus::current(), Some(host.before));
}

#[test]
fn the_input_methods_text_reaches_the_guests_field_only_while_its_stop_holds_the_keys() {
    let host = Host::new();
    let commit = |text: &str| Event::Ime(Ime::Commit(text.into()));
    host.embed.foreign_surface().arrive(false);
    assert_eq!(
        focus::current(),
        None,
        "precondition: the host's keys are elsewhere"
    );

    assert!(!host.embed.on_event(&commit("x")));
    assert!(!host.embed.dispatch_overlays(&commit("x")));
    assert_eq!(host.embed.title(), "ab pressed 0", "the commit stays out");

    focus::request(host.stop);
    host.embed.on_event(&commit("c"));
    assert!(
        host.embed.title().contains('c'),
        "the commit reaches the field: {}",
        host.embed.title()
    );
}

#[test]
fn a_guest_that_panics_takes_its_caret_its_stop_and_its_controls_with_it() {
    let host = Host::new();
    host.tab_in();
    host.frame();
    assert!(
        ui_core::text_input().is_some(),
        "precondition: its field has the caret"
    );

    host.embed.on_event(&Event::PointerPressed {
        x: 1.0,
        y: 1.0,
        button: PointerButton::Secondary,
        source: PointerSource::Mouse,
    });
    assert!(host.embed.panic().is_some(), "precondition: it panicked");
    assert_eq!(ui_core::text_input(), None);
    assert!(
        host.embed.focus_stop_id().is_none(),
        "its stop is withdrawn"
    );
    assert!(
        focus::exposed()
            .iter()
            .all(|exposed| exposed.id != host.stop)
    );
    assert!(
        snapshot(&host.frame())
            .iter()
            .all(|node| node.role != Role::Button),
        "nothing of it is read"
    );
}

#[test]
fn a_released_guest_leaves_no_stop_no_caret_and_nothing_to_read() {
    let host = Host::new();
    host.tab_in();
    let frame = host.frame();
    let inside = host.embed.foreign_surface();
    let Host { embed, stop, .. } = host;
    drop(embed);

    assert_eq!(ui_core::text_input(), None);
    assert!(focus::exposed().iter().all(|exposed| exposed.id != stop));
    assert!(
        snapshot(&frame)
            .iter()
            .all(|node| node.role != Role::Button),
        "a frame still marked as its reads nothing of it"
    );
    assert!(
        inside.access_snapshot().is_empty(),
        "nor does a share of it kept past it"
    );
}

fn named(key: NamedKey) -> Event {
    Event::KeyPressed {
        key: Key::Named(key),
        modifiers: ModifiersState::default(),
        unmodified: None,
    }
}

fn press_at(x: f64, y: f64) -> Event {
    Event::PointerPressed {
        x,
        y,
        button: PointerButton::Primary,
        source: PointerSource::Mouse,
    }
}

/// A host dialog around the guest: what its dismiss stack asks of the focused control is answered inside the guest, so the keys reach the guest's controls rather than closing the dialog.
#[test]
fn enter_and_escape_inside_the_guest_reach_it_rather_than_the_hosts_dialog() {
    let host = Host::new();
    let decided = Rc::new(RefCell::new(Vec::new()));
    let (dismissed, confirmed) = (Rc::clone(&decided), Rc::clone(&decided));
    let _dialog = ui_core::DismissRegistration::confirmable(
        Rc::new(move || dismissed.borrow_mut().push("dismissed")),
        Rc::new(move || confirmed.borrow_mut().push("confirmed")),
    );
    let inside = host.embed.foreign_surface();
    host.tab_in();
    assert!(
        focus::text_entry_focused(),
        "the host hears the guest's field has the keys"
    );

    let escape = named(NamedKey::Escape);
    assert_eq!(ui_core::dispatch_overlays(&escape), EventResult::Ignored);
    host.embed.dispatch_overlays(&escape);
    host.embed.on_event(&escape);
    assert!(
        !read(&inside.access_snapshot(), Role::TextInput).focused,
        "Escape reaches the field, which lets the keyboard go"
    );

    host.tab_in();
    host.embed.on_event(&tab(false));
    let enter = named(NamedKey::Enter);
    assert_eq!(ui_core::dispatch_overlays(&enter), EventResult::Ignored);
    host.embed.on_event(&enter);
    assert_eq!(
        host.embed.title(),
        "ab pressed 1",
        "Enter presses the guest's button"
    );
    assert!(
        decided.borrow().is_empty(),
        "and neither closes the dialog: {decided:?}"
    );
}

#[test]
fn only_a_press_on_the_guest_gives_its_stop_the_hosts_focus() {
    let host = Host::new();
    focus::request(host.before);

    host.embed.on_event(&press_at(-10.0, 50.0));
    assert_eq!(
        focus::current(),
        Some(host.before),
        "a press beside it, forwarded for its overlays"
    );

    host.embed.on_event(&press_at(10.0, 10.0));
    assert_eq!(focus::current(), Some(host.stop));
}

#[test]
fn a_box_event_for_a_surface_the_guest_no_longer_draws_reaches_nothing() {
    let host = Host::new();
    let stale = Event::InSurface {
        surface: u64::MAX >> 33,
        event: Box::new(Event::FocusLeftBoxes),
    };
    host.embed.foreign_surface().arrive(false);
    assert!(!host.embed.dispatch_overlays(&stale));
    assert!(!host.embed.on_event(&stale));
    assert!(
        host.embed.foreign_surface().holds_focus(),
        "nothing reached the guest"
    );

    let own = Event::InSurface {
        surface: host.embed.boundary().key(),
        event: Box::new(Event::FocusLeftBoxes),
    };
    assert!(host.embed.on_event(&own));
    assert!(
        !host.embed.foreign_surface().holds_focus(),
        "its own reaches it"
    );
}
