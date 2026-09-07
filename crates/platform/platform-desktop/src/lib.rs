//! The desktop backend: the winit runner, plus the accessibility, clipboard, dialog and colour-scheme integrations a desktop expects.

#![warn(rustdoc::broken_intra_doc_links)]

#[cfg(feature = "a11y")]
mod accessibility;
#[cfg(feature = "clipboard")]
mod clipboard;
// D-Bus, and only reached on Linux: winit answers the colour-scheme question itself on Windows and macOS. The gate belongs to this module alone — `zbus` is the one dependency declared for Linux only, so a `mod` that drifts above this line takes the gate with it.
#[cfg(all(target_os = "linux", feature = "system-theme"))]
mod color_scheme;
#[cfg(feature = "dialogs")]
mod dialogs;
mod paths;
pub mod platform;

#[cfg(feature = "clipboard")]
pub use clipboard::DesktopClipboard;
#[cfg(feature = "dialogs")]
pub use dialogs::DesktopFileDialogs;
pub use paths::DesktopPathsProvider;
pub use platform::{WinitPlatform, request_dynamic_surface};
// Re-exported from the shared winit backend, so desktop consumers get the runner and window type from one crate.
pub use platform_winit::WinitWindow;
