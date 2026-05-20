pub mod app_context;
pub mod config;
pub mod prefs;
pub mod window_signals;

#[cfg(feature = "runtime")]
pub mod app;
#[cfg(feature = "runtime")]
pub mod runner;

pub use app_context::AppCtx;
pub use config::{RendererBackend, RendererConfig, RsxConfig};
pub use prefs::UserPrefs;
pub use window_signals::WindowSignals;

#[cfg(feature = "runtime")]
pub use app::App;
pub use layout_core::{AlignItems, AvailableSpace, JustifyContent, LayoutStyle};
#[cfg(feature = "runtime")]
pub use platform_core::{Event, ScrollDelta, WindowConfig};
pub use reactive_core::{
    Effect, Memo, ReadSignal, RwSignal, WriteSignal, batch, create_effect, create_memo,
    create_rw_signal, create_signal,
};
#[cfg(feature = "runtime")]
pub use renderer_core::{
    BorderRadius, Color, FillRule, FillStyle, ImageData, ImageFilter, LineCap, LineJoin, LineStyle,
    PathData, PathStyle, PathVerb, Point, Rect as Bounds, RectStyle, Stroke, TextStyle,
};
pub use services_core::{Scope, ServiceRegistry, inject, provide, try_inject, with_service};
pub use ui_core::{
    Button, ClipGroup, Component, ComponentTree, EventResult, Image, IntoView, Label, LayoutLeaf,
    Line, Path, Rect, SubtreeHandle, SubtreeSlot, Text, TranslateGroup, View, WidgetCtx,
    compute_layout, new_container, register_leaf, with_context,
};

#[cfg(feature = "runtime")]
pub use runner::run_app_with_name;

#[cfg(feature = "runtime")]
#[macro_export]
macro_rules! run_app {
    ($config:expr, $app:expr) => {
        $crate::run_app_with_name($config, $app, env!("CARGO_PKG_NAME"))
    };
}
