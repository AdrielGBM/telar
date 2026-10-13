//! A hot-reload guest whose press handler panics, exporting what `app!` exports under `telar/hot-reload`: the `hot_panic` test in `telar` loads it into a host with its own copy of std.
//!
//! Its tree is a themed panel around a line of text that counts the moves it was handed, so mounting, painting and moving over it reach what an app's would: the reactive runtime, layout, the theme, the text measurer and the input registries.

use telar::{
    App, AppCtx, Component, Event, EventResult, LayoutItem, LayoutStyle, RectStyle, RenderNode,
    RwSignal, StyledContainer, Text, TextStyle, box_item, signal, use_theme_tokens,
};

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
        let panel = StyledContainer::new(
            LayoutStyle::new().padding_all(8.0),
            |_| RectStyle::filled(use_theme_tokens().surface_alt(), 4.0),
            vec![box_item(label)],
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
            Event::PointerPressed { .. } => {
                panic!("RUNTIME is already borrowed by the press handler");
            }
            Event::PointerMoved { .. } => {
                self.moves.update(|moves| *moves += 1);
                EventResult::Handled
            }
            _ => EventResult::Ignored,
        }
    }
}

/// Asks for a redraw every frame, so the host can tell whether its frame reached the guest.
struct Fixture;

impl App for Fixture {
    fn root(&self) -> Box<dyn Component> {
        Box::new(PanicsOnPress::new())
    }

    fn on_frame(&mut self, ctx: &mut AppCtx) {
        ctx.request_redraw();
    }
}

/// # Safety
/// Called only by a host built against the same telar, through the table `load_hot_app` checks.
#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn _rsx_hot_create_app() -> telar::hot::GuestResult<Box<dyn App>> {
    telar::hot::guest::create_app(|| Box::new(Fixture))
}

telar::__hot_guest_api!(telar::__hot_guest_exports);
