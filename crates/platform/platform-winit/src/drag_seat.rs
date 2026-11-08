//! The seat's data device every window of one event loop shares: where OS drags start, where they are offered, and where drops arrive. Wayland's is driven here; on every other window system it is inert, and a drag never starts.

use std::sync::Arc;

use platform_core::{DropAnswer, Event, OutgoingDrag};
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

#[cfg(all(unix, not(target_os = "android"), not(target_vendor = "apple")))]
#[path = "drag_seat/wayland.rs"]
mod device;

#[cfg(not(all(unix, not(target_os = "android"), not(target_vendor = "apple"))))]
#[path = "drag_seat/inert.rs"]
mod device;

/// Shared by every window of one event loop, the way the window system's seat is.
#[derive(Clone, Default)]
pub struct DragSeat {
    device: Option<&'static device::Device>,
}

impl DragSeat {
    /// A seat for a window system with no drag support wired yet.
    pub fn none() -> Self {
        Self::default()
    }

    /// The seat of the window system `event_loop` runs on. `wake` asks the loop for another turn once a transfer finishes on a thread of its own.
    pub fn connect(event_loop: &ActiveEventLoop, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            device: device::Device::connect(event_loop, wake),
        }
    }

    /// Lets drags reach `window`, and start from it.
    pub fn attach(&self, window: &Window) {
        if let Some(device) = self.device {
            device.attach(window);
        }
    }

    /// Stops drags reaching `window`, which is closing.
    pub fn detach(&self, window: WindowId) {
        if let Some(device) = self.device {
            device.detach(window);
        }
    }

    /// What happened to drags since the last call, each event for the window it concerns. Called once per turn of the loop.
    pub fn dispatch(&self) -> Vec<(WindowId, Event)> {
        self.device
            .map(device::Device::dispatch)
            .unwrap_or_default()
    }

    pub(crate) fn start_drag(&self, window: WindowId, drag: OutgoingDrag) -> bool {
        self.device
            .is_some_and(|device| device.start_drag(window, drag))
    }

    pub(crate) fn answer_drag(&self, window: WindowId, answer: Option<DropAnswer>) {
        if let Some(device) = self.device {
            device.answer_drag(window, answer);
        }
    }

    pub(crate) fn show_drag_icon(&self, shown: bool) {
        if let Some(device) = self.device {
            device.show_drag_icon(shown);
        }
    }
}
