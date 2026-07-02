mod android;
mod desktop;
mod font_config;
mod handler;
mod hot_host;

const FRAME_BUDGET: std::time::Duration = std::time::Duration::from_nanos(1_000_000_000 / 60);

#[cfg(target_os = "android")]
pub use android::run_android_app_with_name;
#[cfg(not(target_os = "android"))]
pub use desktop::run_app_with_name;
#[cfg(all(feature = "dev", not(target_os = "android")))]
pub use hot_host::run_hot_reload_host;
