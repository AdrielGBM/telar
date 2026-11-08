//! Where no data device is wired yet: there is never one to connect to, so no drag starts and none arrives.

use std::sync::Arc;

use platform_core::{DropAnswer, Event, OutgoingDrag};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

pub(super) enum Device {}

impl Device {
    pub(super) fn connect(
        _event_loop: &ActiveEventLoop,
        _wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Option<&'static Self> {
        None
    }

    pub(super) fn attach(&self, _window: &Window) {
        match *self {}
    }

    pub(super) fn detach(&self, _window: WindowId) {
        match *self {}
    }

    pub(super) fn dispatch(&self) -> Vec<(WindowId, Event)> {
        match *self {}
    }

    pub(super) fn start_drag(&self, _window: WindowId, _drag: OutgoingDrag) -> bool {
        match *self {}
    }

    pub(super) fn answer_drag(&self, _window: WindowId, _answer: Option<DropAnswer>) {
        match *self {}
    }

    pub(super) fn show_drag_icon(&self, _shown: bool) {
        match *self {}
    }
}
