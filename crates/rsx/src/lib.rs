pub mod config;

#[cfg(feature = "runtime")]
pub mod app_config;
#[cfg(feature = "runtime")]
pub mod app_context;
#[cfg(feature = "runtime")]
pub mod prefs;
#[cfg(feature = "runtime")]
pub mod window_signals;

#[cfg(feature = "runtime")]
pub mod app;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub mod paths;
#[cfg(feature = "runtime")]
pub mod runner;

pub use config::{RendererBackend, compile_time_backend};

#[cfg(feature = "runtime")]
pub use app_config::AppConfig;
#[cfg(feature = "runtime")]
pub use app_context::AppCtx;
#[cfg(feature = "runtime")]
pub use prefs::UserPrefs;
#[cfg(feature = "runtime")]
pub use window_signals::WindowSignals;

#[cfg(feature = "runtime")]
pub use app::App;
pub use geometry_core::{Point, Rect, Size};
pub use layout_core::{
    AlignItems, AutoTrack, AvailableSpace, JustifyContent, LayoutError, LayoutStyle, SizeDimension,
    TemplateTrack,
};
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use paths::DesktopPathsProvider;
#[cfg(feature = "runtime")]
pub use platform_core::{Event, ScrollDelta, WindowConfig};
pub use reactive_core::{
    Effect, Memo, ReadSignal, RwSignal, WriteSignal, batch, create_effect, create_memo,
    create_rw_signal, create_signal,
};
#[cfg(feature = "runtime")]
pub use renderer_core::{
    BorderRadius, Color, DrawCommand, FillRule, FrameStylePool, Gradient, GradientKind,
    GradientStop, GradientStops, ImageData, ImageFilter, LineCap, LineJoin, LineStyle, Paint,
    PathData, PathStyle, PathVerb, RectStyle, Shadow, Stroke, StyleHandle, TextStyle,
};
#[cfg(feature = "runtime")]
pub use rsx_devtools::{DevAction, DevPlugin};
pub use services_core::AppPathsProvider;
pub use services_core::{Scope, ServiceRegistry, inject, provide, try_inject, with_service};
pub use theme_core::{Theme, set_theme, use_theme};
pub use ui_core::{
    Button, ButtonStyle, Canvas, Component, ComponentList, Container, EventResult, Group, Image,
    LayoutItem, Line, NodeId, NodeVec, Path, RectView, RenderNode, ScrollArea, ScrollablePage,
    ScrollbarStyle, Text, WidgetCtx, compute_layout, mark_dirty, new_container, new_leaf,
    track_layout, update_style, with_context,
};

#[cfg(all(feature = "runtime", target_os = "android"))]
pub use platform_android::AndroidApp;
#[cfg(all(feature = "runtime", target_os = "android"))]
pub use runner::run_android_app_with_name;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::run_app_with_name;

#[cfg(all(feature = "runtime", not(target_os = "android")))]
#[macro_export]
macro_rules! run_app {
    ($config:expr, $app:expr) => {
        $crate::run_app_with_name(
            $crate::AppConfig::from($config),
            $app,
            env!("CARGO_PKG_NAME"),
        )
    };
}

#[cfg(all(feature = "runtime", not(target_os = "android")))]
#[macro_export]
macro_rules! app {
    ($setup:block, $config:expr, $app:expr) => {
        pub fn run() {
            $setup
            $crate::run_app_with_name(
                $crate::AppConfig::from($config),
                $app,
                env!("CARGO_PKG_NAME"),
            )
        }
    };
}

#[cfg(all(feature = "runtime", target_os = "android"))]
#[macro_export]
macro_rules! app {
    ($setup:block, $config:expr, $app:expr) => {
        #[unsafe(no_mangle)]
        fn android_main(android_app: $crate::AndroidApp) {
            $setup
            $crate::run_android_app_with_name(
                $crate::AppConfig::from($config),
                $app,
                env!("CARGO_PKG_NAME"),
                android_app,
            );
        }
    };
}

#[macro_export]
macro_rules! children {
    ($($item:expr),* $(,)?) => {
        vec![$(Box::new($item) as Box<dyn $crate::LayoutItem>),*]
    }
}
