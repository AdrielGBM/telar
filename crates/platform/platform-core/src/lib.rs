pub mod error;
pub mod event;
pub mod window;

pub use error::PlatformError;
pub use event::{Event, Key, ModifiersState, NamedKey, PointerButton, PointerSource, ScrollDelta};
pub use window::{
    EventHandler, FullscreenMode, MultiSurfacePlatform, Platform, SurfaceId, Window, WindowConfig,
    WindowPosition,
};
