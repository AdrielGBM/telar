pub mod config;
pub mod context;
pub mod prefs;

#[cfg(feature = "runtime")]
pub mod app;
#[cfg(feature = "runtime")]
pub mod runner;

pub use config::{RendererBackend, RendererConfig, RsxConfig};
pub use context::AppContext;
pub use prefs::UserPrefs;

#[cfg(feature = "runtime")]
pub use app::{App, BorderRadius, Color, Frame, Rect, Stroke, WindowConfig};
#[cfg(feature = "runtime")]
pub use runner::run_with_name;

#[cfg(feature = "runtime")]
#[macro_export]
macro_rules! run {
    ($config:expr, $app:expr) => {
        $crate::run_with_name($config, $app, env!("CARGO_PKG_NAME"))
    };
}
