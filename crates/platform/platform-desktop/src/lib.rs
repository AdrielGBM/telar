//! The desktop backend: the winit runner, plus the accessibility, clipboard, dialog and colour-scheme integrations a desktop expects.

mod accessibility;
mod clipboard;
// D-Bus, and only reached on Linux: winit answers the colour-scheme question itself on Windows and macOS. The gate belongs to this module alone — `zbus` is the one dependency declared for Linux only, so a `mod` that drifts above this line takes the gate with it.
#[cfg(target_os = "linux")]
mod color_scheme;
mod dialogs;
mod paths;
pub mod platform;

pub use clipboard::DesktopClipboard;
pub use dialogs::DesktopFileDialogs;
pub use paths::DesktopPathsProvider;
pub use platform::{WinitPlatform, request_dynamic_surface};
// Re-exported from the shared winit backend, so desktop consumers get the runner and window type from one crate.
pub use platform_winit::WinitWindow;
