//! The winit pieces the desktop and Android backends share: the window wrapper and the event mapping.

#![warn(rustdoc::broken_intra_doc_links)]

pub mod map;
pub mod window;

pub use map::{
    SurfaceIntent, TouchDrag, map_key, map_modifiers, map_mouse_button, map_named_key,
    map_window_event,
};
pub use window::WinitWindow;
