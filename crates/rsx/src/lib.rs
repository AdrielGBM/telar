pub mod config;

#[cfg(feature = "runtime")]
pub mod app_context;
#[cfg(feature = "runtime")]
pub mod prefs;
#[cfg(feature = "runtime")]
pub mod window_signals;

#[cfg(feature = "runtime")]
pub mod app;
#[cfg(feature = "runtime")]
pub mod runner;

pub use config::{RendererBackend, compile_time_backend};

#[cfg(feature = "runtime")]
pub use app_context::AppCtx;
#[cfg(feature = "runtime")]
pub use prefs::UserPrefs;
#[cfg(feature = "runtime")]
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
    LineCap, LineJoin, LineStyle, LinearGradient, PathData, PathPayload, PathStyle, PathVerb,
    RadialGradient, RectPayload, RectStyle, Shadow, Stroke, TextPayload, TextStyle,
};
#[cfg(feature = "runtime")]
pub use rsx_devtools::{DevAction, DevPlugin};
pub use services_core::{Scope, ServiceRegistry, inject, provide, try_inject, with_service};
pub use theme_core::{set_theme, use_theme};
pub use ui_core::{
    Button, ButtonStyle, ClipGroup, Component, ComponentTree, Container, DrawingArea, EventResult,
    Image, LayoutItem, LayoutLeaf, Line, NodeId, Path, Rect, ScrollArea, Text, TranslateGroup,
    View, WidgetCtx, compute_layout, mark_dirty, new_container, register_leaf, track_layout,
    update_style, with_context,
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
