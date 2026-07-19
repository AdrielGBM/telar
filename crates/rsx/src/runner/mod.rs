mod android;
#[cfg(all(feature = "desktop", not(target_os = "android")))]
mod desktop;
mod font_config;
pub use font_config::set_default_font_family;
#[cfg(not(target_os = "android"))]
mod generic;
mod handler;
mod hot_host;
#[cfg(not(target_os = "android"))]
mod multi;

const FRAME_BUDGET: std::time::Duration = std::time::Duration::from_nanos(1_000_000_000 / 60);

// F4: how long after the last content frame the HW backend keeps issuing 1fps keepalive blits before
// letting the GPU sleep. Covers interactive bursts (scroll/typing gaps) without waking the GPU once a
// second forever on a truly static screen (battery). Real input/redraw events still wake the loop.
const HW_KEEPALIVE_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

#[cfg(target_os = "android")]
pub use android::run_android_app_with_name;
#[cfg(all(feature = "desktop", not(target_os = "android")))]
pub use desktop::{open_window, run_app_windowed, run_app_with_name, run_multi_app_with_name};
#[cfg(not(target_os = "android"))]
pub use generic::run_with_platform;
#[cfg(all(feature = "dev", not(target_os = "android")))]
pub use hot_host::run_hot_reload_host;
#[cfg(not(target_os = "android"))]
pub use multi::{build_surface_handler, run_multi_with_platform};
