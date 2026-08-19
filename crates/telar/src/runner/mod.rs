#[cfg(target_os = "android")]
mod android;
#[cfg(all(feature = "desktop", not(target_os = "android")))]
mod desktop;
#[cfg(any(
    all(feature = "desktop", not(target_os = "android")),
    target_os = "android"
))]
mod dev_window;
mod font_config;
mod frame_thread;
#[cfg(feature = "hardware")]
pub(crate) use font_config::offscreen_hardware_font_config;
pub use font_config::set_default_font_family;
mod generic;
mod handler;
mod host;
#[cfg(all(feature = "dev", not(target_os = "android")))]
mod hot_host;
mod multi;

/// The window an app opens with: `App::window_config` outright replaces whatever the caller passed,
/// including the `[telar.dev.window]` overrides. See [`dev_window::with_dev_overrides`] for why that order.
fn resolved_window<A: crate::app::App + ?Sized>(
    from_config: platform_core::WindowConfig,
    app: &A,
) -> platform_core::WindowConfig {
    app.window_config().unwrap_or(from_config)
}

const FRAME_BUDGET: std::time::Duration = std::time::Duration::from_nanos(1_000_000_000 / 60);

// How long after the last input the HW backend keeps issuing 1fps keepalive blits before letting the GPU
// sleep, on top of the window still being focused. Long, because what it guards against is somebody walking
// away from a window they left open, and what it must not do is punish somebody who paused to read: a screen
// being read produces no frames at all, so timing this from the last *frame* slept while they were still
// there and made the next key press wait for the GPU to clock back up.
const IDLE_GRACE: std::time::Duration = std::time::Duration::from_secs(60);

// The cadence of those keepalive blits. Enforced in `on_redraw` as well as reported by `about_to_wait`, because a platform may call `on_redraw` on every loop turn rather than only when a frame is due.
const HW_KEEPALIVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

// F2: how many command buffers the UI thread keeps to refill instead of allocating a fresh Vec per frame.
// Only the in-flight frame plus the one the render thread is consuming are ever live, so the free-list
// stays tiny; a larger cap would only hold memory.
const COMMAND_BUF_POOL_CAP: usize = 3;

#[cfg(target_os = "android")]
pub use android::run_android_app_with_name;
#[cfg(all(feature = "desktop", not(target_os = "android")))]
pub use desktop::{open_window, run_app_windowed, run_app_with_name};
pub use generic::{run_with_platform, run_with_platform_and_renderer};
pub use host::SurfaceWindow;
#[cfg(all(feature = "dev", not(target_os = "android")))]
pub use hot_host::run_hot_reload_host;
pub use multi::{build_surface_handler, run_multi_with_platform};
