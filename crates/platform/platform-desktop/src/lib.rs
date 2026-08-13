#[cfg(target_os = "linux")]
mod color_scheme;
mod dialogs;
mod paths;
pub mod platform;

pub use dialogs::DesktopFileDialogs;
pub use paths::DesktopPathsProvider;
pub use platform::{WinitPlatform, request_dynamic_surface};
// Re-exported from the shared winit backend so desktop consumers get the runner and its window type from one crate.
pub use platform_winit::WinitWindow;
