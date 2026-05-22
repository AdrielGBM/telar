pub mod app_context;
pub mod config;
pub mod prefs;
pub mod window_signals;

#[cfg(feature = "runtime")]
pub mod app;
#[cfg(feature = "runtime")]
pub mod runner;

pub use app_context::AppCtx;
pub use config::RendererBackend;
pub use prefs::UserPrefs;
pub use window_signals::WindowSignals;

#[cfg(feature = "runtime")]
pub use app::App;
pub use geometry_core::{Point, Rect as Bounds};
pub use layout_core::{
    AlignItems, AvailableSpace, JustifyContent, LayoutError, LayoutStyle, Track,
};
#[cfg(feature = "runtime")]
pub use platform_core::{Event, ScrollDelta, WindowConfig};
pub use reactive_core::{
    Effect, Memo, ReadSignal, RwSignal, WriteSignal, batch, create_effect, create_memo,
    create_rw_signal, create_signal,
};
#[cfg(feature = "runtime")]
pub use renderer_core::{
    BorderRadius, Color, DrawCommand, FillRule, FillStyle, GradientStop, ImageData, ImageFilter,
    LineCap, LineJoin, LineStyle, LinearGradient, PathData, PathStyle, PathVerb, RadialGradient,
    RectStyle, Shadow, Stroke, TextStyle,
};
pub use services_core::{Scope, ServiceRegistry, inject, provide, try_inject, with_service};
pub use ui_core::{
    Button, ClipGroup, Component, ComponentTree, Container, DrawingArea, EventResult, Image, Label,
    LayoutItem, LayoutLeaf, Line, Path, Rect, ScrollArea, Text, TranslateGroup, View, WidgetCtx,
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
