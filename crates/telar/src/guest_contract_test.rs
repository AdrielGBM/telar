//! End-to-end tests of the one guest contract hot reload and embed share, from a host whose global allocator is not its guests': the guests `tests/fixtures/hot-panic-guest` and `tests/fixtures/embed-guest` each mark every block they allocate apart from this binary's, and either side aborts on freeing a block the other made. They build both under `target/tmp`: run them with `cargo test -p telar --features dev,shaper --test guest_contract -- --ignored`.

#![cfg(not(target_arch = "wasm32"))]

#[path = "guest_fixture.rs"]
mod guest_fixture;

#[path = "../tests/fixtures/tagged_alloc.rs"]
mod tagged_alloc;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use telar::hot::{HotApp, load_hot_app};
use telar::{
    AppCtx, AppRuntime, Color, DrawCommand, Event, EventResult, Key, ModifiersState, PointerButton,
    PointerSource, RedrawWaker, Size, SystemPreferences, WindowCommand,
};
use telar_embed::{LoadedEmbed, load_embedded};

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
        assert_eq!(app.take_panic(), None, "precondition: nothing panicked");

        tree.on_event(&pressed(PointerButton::Primary));
        let report = app.take_panic().expect("its panic report crosses");
        assert!(
            report.ends_with("RUNTIME is already borrowed by the press handler"),
            "{report}"
        );
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
        assert!(embed.on_event(&key('t')));
        assert_eq!(
            embed.drain_window_commands(),
            [WindowCommand::SetTitle(
                "embedded here moved 1 times".to_owned()
            )],
            "its window commands cross"
        );
        embed.end_frame();
        embed.motion_tick(web_time::Instant::now());
        assert!(!embed.motion_has_active());

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
