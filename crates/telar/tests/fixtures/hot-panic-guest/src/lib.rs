//! A hot-reload guest whose press handler panics, exporting what `app!` exports under `telar/hot-reload`: the `hot_panic` test in `telar` loads it into a host with its own copy of std.
//!
//! Its tree is a themed panel around a line of text that counts the moves it was handed, a button and an icon nothing baked, so mounting, painting and moving over it reach what an app's would: the reactive runtime, layout, the theme, the text measurer, the input registries, and the slots of the plugins an app draws with. A secondary press adds a face, as a font fetched while an app runs is.

use telar::{
    App, AppCtx, Children, Chord, Component, Event, EventResult, FontAsset, LayoutItem,
    LayoutStyle, PointerButton, RectStyle, RenderNode, RwSignal, StyledContainer, Text, TextStyle,
    WindowConfig, box_item, signal, use_theme_tokens,
};
use telar_components::{ButtonProps, button};
use telar_icons::{IconProps, icon};

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
        let panel = StyledContainer::new(
            LayoutStyle::new().padding_all(8.0),
            |_| RectStyle::filled(use_theme_tokens().surface_alt(), 4.0),
            vec![box_item(label), save, missing],
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

/// # Safety
/// Called only by a host built against the same telar, through the table `load_hot_app` checks.
#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn _rsx_hot_create_app() -> telar::hot::GuestResult<Box<dyn App>> {
    telar::hot::guest::create_app(|| Box::new(Fixture))
}

telar::__hot_guest_api!(telar::__hot_guest_exports);
