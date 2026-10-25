//! End-to-end tests of the one guest contract hot reload and embed share, from a host whose global allocator is not its guests': the guests `tests/fixtures/hot-panic-guest` and `tests/fixtures/embed-guest` each mark every block they allocate apart from this binary's, and either side aborts on freeing a block the other made. They build both under `target/tmp`: run them with `cargo test -p telar --features dev,shaper --test guest_contract -- --ignored`.

#![cfg(not(target_arch = "wasm32"))]

#[path = "guest_fixture.rs"]
mod guest_fixture;

#[path = "../tests/fixtures/tagged_alloc.rs"]
mod tagged_alloc;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use platform_core::AccessNode;
use telar::focus::FocusId;
use telar::hot::{HotApp, load_hot_app};
use telar::{
    AppCtx, AppRuntime, AvailableSpace, Color, Component, ComponentList, Container, DrawCommand,
    Event, EventResult, Ime, Key, LayoutItem, LayoutStyle, ModifiersState, NamedKey, NodeId,
    PointerButton, PointerSource, Rect, RedrawWaker, RenderNode, Size, SystemPreferences,
    Transform, WindowCommand, compute_layout, focus,
};
use telar_embed::{LoadedEmbed, load_embedded};
use ui_core::accessibility::{AccessTarget, perform_action};

#[global_allocator]
static ALLOCATOR: tagged_alloc::Tagged = tagged_alloc::Tagged(0x686f_7374_5f68_6f73);

/// One at a time: each installs the process's services and fonts for the guest it loads.
static SERIAL: Mutex<()> = Mutex::new(());

#[test]
#[ignore = "builds two cdylibs against this checkout: run it with `cargo test -p telar --features dev,shaper --test guest_contract -- --ignored`"]
fn a_hot_guest_with_an_allocator_of_its_own_runs_reloads_and_unloads() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build("hot-panic-guest");
    telar::font_families();
    for _ in 0..3 {
        let mut app = load_hot(&library).expect("the guest loads");
        assert!(app.window_config().is_some(), "its window config crosses");
        app.set_surface_size(Size::new(320.0, 200.0));
        app.set_system_preferences(&SystemPreferences::default());
        let mut tree = app.mount();
        app.relayout();
        let mut handled = None;
        app.in_event_batch(&mut || handled = Some(tree.on_event(&hover())));
        assert_eq!(handled, Some(EventResult::Handled));
        assert_eq!(
            tree.on_event(&pressed(PointerButton::Secondary)),
            EventResult::Handled,
            "the guest adds a face through the host's services"
        );
        app.relayout();
        assert!(runs_a_frame(&mut app), "its frame runs");
        assert!(!tree.frame().is_empty(), "its frame crosses");
        assert!(!tree.frame().is_empty(), "and the next, built on it");
        tree.end_frame();
        let mut walked = Vec::new();
        tree.walk(&mut walked);
        assert!(!walked.is_empty(), "its tree crosses");
        let snapshot = app.hot_snapshot().expect("its state crosses");
        app.hot_restore(&snapshot);
        app.drain_window_commands();
        app.until_next_timer();

        let window = telar::inspect::snapshot(&tree.frame());
        let save = control(&window, focus::Role::Button);
        assert!(
            matches!(
                AccessTarget::of(save.id.expect("a control")),
                AccessTarget::Nested { .. }
            ),
            "the window reads the guest's button under an id of the guest's surface"
        );
        let field = control(&window, focus::Role::TextInput)
            .id
            .expect("a control");
        perform_action(field, false, |event| {
            panic!("the reader's action reaches the guest itself: {event:?}")
        });
        assert!(
            control(
                &telar::inspect::snapshot(&tree.frame()),
                focus::Role::TextInput
            )
            .focused,
            "and moves the guest's focus"
        );
        let input = app
            .text_input()
            .expect("the window hears where the guest's field has its caret");
        assert!(input.caret.height > 0.0, "{input:?}");
        let audit = telar::inspect::audit(&tree.frame(), None);
        assert!(
            audit
                .nodes
                .iter()
                .any(|node| node.role == focus::Role::Button),
            "the inspector reads the guest's own audit"
        );
        assert_eq!(app.take_panic(), None, "precondition: nothing panicked");

        tree.on_event(&pressed(PointerButton::Primary));
        let report = app.take_panic().expect("its panic report crosses");
        assert!(
            report.ends_with("RUNTIME is already borrowed by the press handler"),
            "{report}"
        );
        assert_eq!(app.text_input(), None, "a guest that panicked has no caret");
        drop(tree);
        drop(app);
    }
}

#[test]
#[ignore = "builds two cdylibs against this checkout: run it with `cargo test -p telar --features dev,shaper --test guest_contract -- --ignored`"]
fn an_embedded_guest_with_an_allocator_of_its_own_runs_and_unloads() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build("embed-guest");
    telar::font_families();
    for _ in 0..3 {
        let embed = load_embed(&library, &["embedded".to_owned(), "here".to_owned()])
            .expect("the guest loads");
        assert_eq!(embed.title(), "embedded here", "the launch args cross");
        assert_eq!(embed.id(), "embed-guest");
        assert_eq!(embed.icon(), Some(vec![1, 2, 3, 4]));
        assert_eq!(embed.clear_color(), Some(Color::rgb(0.1, 0.2, 0.3)));
        embed.set_system_preferences(&SystemPreferences::default());
        embed.activate();
        embed.relayout(320.0, 200.0);

        let first = embed.paint();
        assert!(
            first
                .iter()
                .any(|command| matches!(command, DrawCommand::Text { .. })),
            "its text crosses: {first:?}"
        );
        let image = first
            .iter()
            .find_map(|command| match command {
                DrawCommand::Image { data, .. } => Some(Arc::clone(data)),
                _ => None,
            })
            .expect("its image crosses");
        assert_eq!(image.pixels().len(), 4 * 4 * 4, "with its pixels");
        assert_eq!(
            embed.paint().len(),
            first.len(),
            "and the next frame, built on it"
        );

        assert_eq!(label(&first), "moved over 0 times");
        assert!(!embed.dispatch_overlays(&hover()));
        assert!(embed.on_event(&hover()), "events cross");
        embed.relayout_dirty();
        assert_eq!(
            label(&embed.paint()),
            "moved over 1 times",
            "and reach its signals"
        );
        assert_eq!(embed.generation(), embed.generation());
        embed.end_frame();
        embed.motion_tick(web_time::Instant::now());
        assert!(!embed.motion_has_active());
        assert!(
            !embed.on_event(&key('t')),
            "no key reaches it without its stop"
        );
        let host = HostRow::new();
        focus::request(embed.focus_stop(host.node, || Transform::IDENTITY));
        assert!(embed.on_event(&key('t')));
        assert_eq!(
            embed.drain_window_commands(),
            [WindowCommand::SetTitle(
                "embedded here moved 1 times".to_owned()
            )],
            "its window commands cross"
        );

        let wakes = Arc::new(AtomicUsize::new(0));
        let counting = Arc::clone(&wakes);
        let waker = RedrawWaker::new(move || {
            counting.fetch_add(1, Ordering::Relaxed);
        });
        let mut redraw_requested = false;
        let mut ctx = AppCtx::new(&mut redraw_requested, Some(&waker), None, None);
        embed.on_frame(&mut ctx);
        assert!(redraw_requested, "its redraw crosses");
        assert!(
            wakes.load(Ordering::Relaxed) > 0,
            "and it wakes the host's loop"
        );
        let wait = embed.until_next_timer().expect("its timer crosses");
        assert!(wait.as_secs() <= 60, "{wait:?}");
        assert_eq!(embed.panic(), None, "precondition: nothing panicked");

        assert!(!embed.on_event(&pressed(PointerButton::Primary)));
        let panic = embed.panic().expect("its panic report crosses");
        assert_eq!(
            panic.message,
            "RUNTIME is already borrowed by the press handler"
        );
        drop(embed);
    }
}

/// One focus owner across the two runtimes: what the guest reports of its surface is read where the host draws it, a reader's action presses its button, the keyboard walks in and out of it through the host's stop, and its caret and text reach its field from the host's input method only while that stop holds the keys.
#[test]
#[ignore = "builds two cdylibs against this checkout: run it with `cargo test -p telar --features dev,shaper --test guest_contract -- --ignored`"]
fn an_embedded_guests_surface_is_read_acted_on_and_typed_into_where_the_host_draws_it() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build("embed-guest");
    telar::font_families();
    let embed = load_embed(&library, &[]).expect("the guest loads");
    embed.relayout(AT.width, AT.height);
    let host = HostRow::new();
    let stop = embed.focus_stop(host.node, || Transform::IDENTITY);
    let frame = || {
        let drawn = telar_embed::composite(AT, embed.boundary(), embed.paint());
        ComponentList::new(Drawn(RefCell::new(Some(drawn))))
            .commands()
            .clone()
    };

    let window = telar::inspect::snapshot(&frame());
    let field = control(&window, focus::Role::TextInput);
    assert_eq!(
        (field.rect.x, field.rect.y > AT.y),
        (AT.x, true),
        "{field:?}"
    );
    let button = control(&window, focus::Role::Button);
    assert!(matches!(
        AccessTarget::of(button.id.expect("a control")),
        AccessTarget::Nested { surface, .. } if surface == embed.boundary().key()
    ));
    perform_action(button.id.expect("a control"), true, |event| {
        panic!("the reader's action reaches the guest itself: {event:?}")
    });
    assert_eq!(
        embed.drain_window_commands(),
        [WindowCommand::SetTitle("pressed 1".to_owned())],
        "and presses its button"
    );
    assert!(focus::is_focused(stop), "whose stop takes the keys");

    focus::request(host.before);
    focus::focus_next();
    assert!(focus::is_focused(stop));
    frame();
    let caret = ui_core::text_input()
        .expect("Tab hands the guest's field the caret, which the host's input method hears")
        .caret;
    assert!(
        AT.contains(caret.x, caret.y),
        "where the host shows it: {caret:?}"
    );
    let decided = std::rc::Rc::new(std::cell::Cell::new(false));
    let (dismissed, confirmed) = (decided.clone(), decided.clone());
    let _dialog = ui_core::DismissRegistration::confirmable(
        std::rc::Rc::new(move || dismissed.set(true)),
        std::rc::Rc::new(move || confirmed.set(true)),
    );
    for key in [NamedKey::Enter, NamedKey::Escape] {
        assert_eq!(
            ui_core::dispatch_overlays(&named(key.clone())),
            EventResult::Ignored,
            "the guest's field keeps {key:?} from the host's dialog"
        );
    }
    assert!(!decided.get());
    let commit = Event::Ime(Ime::Commit("c".into()));
    assert!(embed.on_event(&commit));
    let typed = control(&telar::inspect::snapshot(&frame()), focus::Role::TextInput).name;
    assert!(typed.contains('c'), "the commit reaches the field: {typed}");

    assert!(embed.on_event(&tab(false)));
    assert!(focus::is_focused(stop), "Tab moves on inside");
    assert!(embed.on_event(&tab(false)));
    assert_eq!(focus::current(), Some(host.after), "and out past its last");
    assert!(
        !embed.foreign_surface().holds_focus(),
        "leaving nothing focused inside"
    );
    assert!(!embed.on_event(&commit), "a commit no longer reaches it");

    embed.on_event(&pressed(PointerButton::Primary));
    assert!(embed.panic().is_some(), "precondition: it panicked");
    assert!(embed.focus_stop_id().is_none(), "its stop is withdrawn");
    assert_eq!(ui_core::text_input(), None);
    assert!(
        telar::inspect::snapshot(&frame())
            .iter()
            .all(|node| node.role != focus::Role::Button),
        "and nothing of it is read"
    );
}

#[test]
#[ignore = "builds two cdylibs against this checkout: run it with `cargo test -p telar --features dev,shaper --test guest_contract -- --ignored`"]
fn an_embedded_guest_is_refused_as_a_hot_reload_library() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build("embed-guest");
    let refused = load_hot(&library)
        .err()
        .expect("an embedded guest is no app to swap in");
    assert!(
        refused
            .to_string()
            .ends_with("this is an embedded guest, not a hot-reload library"),
        "{refused}"
    );
}

#[test]
#[ignore = "builds two cdylibs against this checkout: run it with `cargo test -p telar --features dev,shaper --test guest_contract -- --ignored`"]
fn a_hot_reload_library_is_refused_as_an_embedded_guest() {
    let _serial = SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
    let library = build("hot-panic-guest");
    let refused = load_embed(&library, &[])
        .err()
        .expect("a hot-reload library is no guest to embed");
    assert!(
        refused
            .to_string()
            .ends_with("this is a hot-reload library, not an embedded guest"),
        "{refused}"
    );
}

/// Where the host draws an embedded guest, in a box of its own tree.
const AT: Rect = Rect {
    x: 40.0,
    y: 20.0,
    width: 200.0,
    height: 100.0,
};

/// A host row laid out and drawn: a focusable before the box an embedded guest is drawn in at [`AT`] and one after it, by their focus ids, and that box's node.
struct HostRow {
    _row: Container,
    before: FocusId,
    node: NodeId,
    after: FocusId,
}

impl HostRow {
    fn new() -> Self {
        telar::reset_layout_runtime();
        focus::clear();
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
            focus::register_with_role(id, focus::FocusKind::Widget, node, focus::Role::Group);
            id
        };
        Self {
            _row: row,
            before: stop_at(nodes[0]),
            node: nodes[1],
            after: stop_at(nodes[2]),
        }
    }
}

/// A host's tree that draws one node once.
struct Drawn(RefCell<Option<RenderNode>>);

impl Component for Drawn {
    fn view(&self) -> RenderNode {
        self.0.borrow_mut().take().unwrap_or(RenderNode::Empty)
    }
}

/// The first control of `role` in `nodes`.
fn control(nodes: &[AccessNode], role: focus::Role) -> AccessNode {
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

/// The text the embed fixture's label draws in `frame`.
fn label(frame: &[DrawCommand]) -> String {
    frame
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text { text, .. } => Some(text.to_string()),
            _ => None,
        })
        .expect("the label draws")
}

/// Whether the guest's frame ran: the hot fixture asks for a redraw every time it does.
fn runs_a_frame(app: &mut dyn AppRuntime) -> bool {
    let mut redraw_requested = false;
    let mut ctx = AppCtx::new(&mut redraw_requested, None, None, None);
    app.on_frame(&mut ctx);
    redraw_requested
}

fn hover() -> Event {
    Event::PointerMoved {
        x: 10.0,
        y: 10.0,
        source: PointerSource::Mouse,
    }
}

fn pressed(button: PointerButton) -> Event {
    Event::PointerPressed {
        x: 10.0,
        y: 10.0,
        button,
        source: PointerSource::Mouse,
    }
}

fn key(character: char) -> Event {
    Event::KeyPressed {
        key: Key::Char(character),
        modifiers: ModifiersState::default(),
        unmodified: None,
    }
}

fn build(fixture: &str) -> PathBuf {
    guest_fixture::build_guest_fixture(Path::new(env!("CARGO_TARGET_TMPDIR")), fixture)
}

fn load_hot(library: &Path) -> Result<HotApp, Box<dyn std::error::Error>> {
    // SAFETY: the fixture was just built from this checkout, against the telar this test links.
    unsafe { load_hot_app(library) }
}

fn load_embed(library: &Path, args: &[String]) -> Result<LoadedEmbed, Box<dyn std::error::Error>> {
    // SAFETY: the fixture was just built from this checkout, against the telar-embed this test links.
    unsafe { load_embedded(library, args) }
}

fn named(key: NamedKey) -> Event {
    Event::KeyPressed {
        key: Key::Named(key),
        modifiers: ModifiersState::default(),
        unmodified: None,
    }
}
