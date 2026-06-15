#[cfg(target_os = "android")]
mod paths;
#[cfg(target_os = "android")]
pub mod platform;
#[cfg(target_os = "android")]
pub mod window;

#[cfg(target_os = "android")]
pub use android_activity::AndroidApp;
#[cfg(target_os = "android")]
pub use paths::AndroidPathsProvider;
#[cfg(target_os = "android")]
pub use platform::AndroidPlatform;
#[cfg(target_os = "android")]
pub use window::AndroidWindow;
