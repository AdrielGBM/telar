//! The desktop backend: the winit runner, plus the accessibility, clipboard, dialog, URI-opening and system-preference integrations a desktop expects.

#![warn(rustdoc::broken_intra_doc_links)]

#[cfg(feature = "a11y")]
mod accessibility;
#[cfg(feature = "clipboard")]
mod clipboard;
#[cfg(feature = "dialogs")]
mod dialogs;
mod paths;
pub mod platform;
mod system_preferences;
mod uri;

#[cfg(feature = "clipboard")]
pub use clipboard::DesktopClipboard;
#[cfg(feature = "dialogs")]
pub use dialogs::DesktopFileDialogs;
pub use paths::DesktopPathsProvider;
pub use platform::{WinitPlatform, request_dynamic_surface};
pub use uri::DesktopUriOpener;
// Re-exported from the shared winit backend, so desktop consumers get the runner and window type from one crate.
pub use platform_winit::WinitWindow;
