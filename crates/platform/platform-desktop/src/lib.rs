#[cfg(target_os = "linux")]
mod color_scheme;
mod paths;
pub mod platform;

pub use paths::DesktopPathsProvider;
pub use platform::WinitPlatform;
// Re-exported from the shared winit backend so desktop consumers get the runner and its window type from one crate.
pub use platform_winit::WinitWindow;
