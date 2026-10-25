//! A hot-reload guest the `hot_panic` test in `telar` loads into a host with its own copy of std: its tree reaches what an app's does (reactive runtime, layout, theme, text measurer, input registries, plugin slots, baked assets) from an allocator of its own, and its pointer events panic, add a face, leave a task running, start a thread with its own std or leave a worker that cannot be stopped, as each test needs.

use telar::{
    App, AppCtx, Children, Chord, Component, Event, EventResult, FontAsset, LayoutItem,
    LayoutStyle, PointerButton, RectStyle, RenderNode, RwSignal, StyledContainer, Text, TextStyle,
    WindowConfig, box_item, signal, spawn_task, use_theme_tokens,
};
use telar_components::{ButtonProps, button};
use telar_icons::{IconProps, icon};
use telar_rsx_fixture::{GreetingCardProps, greeting_card};

#[path = "../../tagged_alloc.rs"]
mod tagged_alloc;

/// An allocator of its own, marked apart from the host's, so a block that crosses the contract owned and is freed on the other side stops the process.
#[global_allocator]
static ALLOCATOR: tagged_alloc::Tagged = tagged_alloc::Tagged(0x686f_745f_6775_6573);

/// How long the task an auxiliary press or a secondary release leaves running takes: the `hot_panic` test waits at least this long for the guest to unload.
const TASK_RUNS_FOR: std::time::Duration = std::time::Duration::from_millis(300);

const ARRIVING_FACE: &[u8] =
    include_bytes!("../../../../../renderer/renderer-text/test-fonts/TelarTest.ttf");

struct PanicsOnPress {
    content: Box<dyn LayoutItem>,
    moves: RwSignal<u32>,
}

impl PanicsOnPress {
    fn new() -> Self {
        let moves = signal(0);
        let label = Text::new(
            move || format!("moved over {} times", moves.get()),
            LayoutStyle::new(),
            || TextStyle::new(14.0, use_theme_tokens().ink()),
        )
        .expect("the label builds");
        let save = button(
            ButtonProps::props().label("Save").build(),
            Children::default(),
        )
        .expect("the button builds");
        let missing = icon(
            IconProps::props().name("fixture:unbaked").build(),
            Children::default(),
        )
        .expect("the icon builds");
        let card = greeting_card(
            GreetingCardProps::props().name("Ada").build(),
            Children::default(),
        )
        .expect("the card builds");
        let panel = StyledContainer::new(
            LayoutStyle::new().padding_all(8.0),
            |_| RectStyle::filled(use_theme_tokens().surface_alt(), 4.0),
            vec![box_item(label), save, missing, card],
        )
        .expect("the panel builds");
        Self {
            content: box_item(panel),
            moves,
        }
    }
}

impl Component for PanicsOnPress {
    fn view(&self) -> RenderNode {
        self.content.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::PointerPressed {
                button: PointerButton::Secondary,
                ..
            } => {
                telar_renderer_text::fonts::add_faces(vec![
                    FontAsset::embedded(ARRIVING_FACE).named("Arriving Face"),
                ]);
                EventResult::Handled
            }
            Event::PointerPressed { button, .. } => {
                if *button == PointerButton::Auxiliary {
                    spawn_task(|| std::thread::sleep(TASK_RUNS_FOR), |()| {});
                }
                panic!("RUNTIME is already borrowed by the press handler");
            }
            Event::PointerReleased {
                button: PointerButton::Primary,
                ..
            } => {
                telar_reactive_local::thread_exit::on_quiesce(stop_a_worker_that_will_not);
                EventResult::Handled
            }
            Event::PointerReleased {
                button: PointerButton::Secondary,
                ..
            } => {
                spawn_task(|| std::thread::sleep(TASK_RUNS_FOR), |()| {});
                EventResult::Handled
            }
            Event::PointerReleased {
                button: PointerButton::Auxiliary,
                ..
            } => {
                let _ = std::thread::spawn(|| ()).join();
                EventResult::Handled
            }
            Event::PointerMoved { .. } => {
                self.moves.update(|moves| *moves += 1);
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }
}

fn stop_a_worker_that_will_not() {
    std::panic::panic_any("a worker that will not stop");
}

/// Asks for a redraw every frame, so the host can tell whether its frame reached the guest, and titles its window with a shortcut, so the host can read which keyboard the guest shows it for.
struct Fixture;

impl App for Fixture {
    fn root(&self) -> Box<dyn Component> {
        Box::new(PanicsOnPress::new())
    }

    fn on_frame(&mut self, ctx: &mut AppCtx) {
        ctx.request_redraw();
    }

    fn window_config(&self) -> Option<WindowConfig> {
        let save = Chord::parse("Mod+S").expect("the shortcut parses");
        Some(WindowConfig {
            title: save.to_string(),
            ..WindowConfig::default()
        })
    }
}

telar::__hot_guest!(|| -> Box<dyn App> { Box::new(Fixture) });
