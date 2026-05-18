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
pub use app::{App, Frame};
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
    PathData, PathStyle, PathVerb, Point, Rect, RectStyle, RendererError, Stroke, TextStyle,
};
pub use services_core::{Scope, ServiceRegistry, inject, provide, try_inject, with_service};
pub use ui_core::{
    Button, Component, ComponentTree, EventResult, IntoView, Label, View, WidgetCtx,
    compute_layout, new_container, register_leaf, with_context,
};

#[cfg(feature = "runtime")]
pub use runner::run_with_name;

#[cfg(feature = "runtime")]
#[macro_export]
macro_rules! run {
    ($config:expr, $app:expr) => {
        $crate::run_with_name($config, $app, env!("CARGO_PKG_NAME"))
    };
}
