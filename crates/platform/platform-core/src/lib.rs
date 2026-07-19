pub mod error;
pub mod event;
pub mod window;
pub mod window_command;

pub use error::PlatformError;
pub use event::{Event, Key, ModifiersState, NamedKey, PointerButton, PointerSource, ScrollDelta};
pub use window::{
    EventHandler, FullscreenMode, MultiSurfacePlatform, Platform, SurfaceId, Window, WindowConfig,
    WindowPosition,
};
pub use window_command::{
    WindowCommand, WindowCommandContext, WindowCommandGuard, push_window_command,
    take_window_commands,
};
