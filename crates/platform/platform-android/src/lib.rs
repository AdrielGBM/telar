//! The Android backend: an `android-activity` loop, plus the platform facts the NDK answers for.

#![warn(rustdoc::broken_intra_doc_links)]

#[cfg(target_os = "android")]
mod adpf;
#[cfg(target_os = "android")]
pub mod fonts;
#[cfg(target_os = "android")]
mod paths;
#[cfg(target_os = "android")]
pub mod platform;
#[cfg(target_os = "android")]
mod sys_prop;

#[cfg(target_os = "android")]
pub use adpf::AdpfSession;
#[cfg(target_os = "android")]
pub use android_activity::AndroidApp;
#[cfg(target_os = "android")]
pub use paths::AndroidPathsProvider;
#[cfg(target_os = "android")]
pub use platform::AndroidPlatform;
#[cfg(target_os = "android")]
pub use sys_prop::read_sys_prop;
// The android backend reuses the shared winit window wrapper; kept under the AndroidWindow name for callers.
#[cfg(target_os = "android")]
pub use platform_winit::WinitWindow as AndroidWindow;
