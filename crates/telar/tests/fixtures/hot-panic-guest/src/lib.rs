//! A hot-reload guest whose press handler panics, exporting what `app!` exports under `telar/hot-reload`: the `hot_panic` test in `telar` loads it into a host with its own copy of std.

use telar::{App, AppCtx, Component, Event, EventResult, RenderNode};

struct PanicsOnPress;

impl Component for PanicsOnPress {
    fn view(&self) -> RenderNode {
        RenderNode::group([])
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if matches!(event, Event::PointerPressed { .. }) {
            panic!("RUNTIME is already borrowed by the press handler");
        }
        EventResult::Ignored
    }
}

/// Asks for a redraw every frame, so the host can tell whether its frame reached the guest.
struct Fixture;

impl App for Fixture {
    fn root(&self) -> Box<dyn Component> {
        Box::new(PanicsOnPress)
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
