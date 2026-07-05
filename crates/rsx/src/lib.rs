mod macros;

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
#[cfg(all(feature = "dev", not(target_os = "android")))]
pub mod hot;
#[cfg(feature = "dev")]
pub mod hot_state;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub mod paths;
#[cfg(feature = "runtime")]
pub mod runner;

pub use config::RendererBackend;

#[cfg(feature = "runtime")]
mod preview_runner;

#[cfg(feature = "runtime")]
pub use preview_runner::PreviewEntry;

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
#[cfg(feature = "runtime")]
pub use devtools_core::{DevAction, DevPlugin};
#[cfg(feature = "runtime")]
pub use geometry_core::{ObjectFit, Point, Rect, Transform};
#[cfg(feature = "runtime")]
pub use layout_core::{
    AlignItems, AvailableSpace, JustifyContent, LayoutError, LayoutStyle, SizeDimension,
    TemplateTrack,
};
// Always-on, no feature gate (D2 in docs/animations.md): kernel functionality, not an opt-in module. The transpiler emits `motion::Animated`/`motion::tween`/`motion::spring`/`motion::Easing` paths against this re-export.
pub use motion_core as motion;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use paths::DesktopPathsProvider;
#[cfg(feature = "runtime")]
pub use platform_core::{Event, FullscreenMode, ScrollDelta, WindowConfig, WindowPosition};
#[cfg(feature = "runtime")]
pub use reactive_core::{
    Effect, Memo, ReadSignal, RwSignal, batch, begin_batch, effect, end_batch, memo, reset_runtime,
    signal,
};
#[cfg(all(feature = "runtime", feature = "svg"))]
pub use renderer_assets::{SvgData, SvgError};
#[cfg(feature = "runtime")]
pub use renderer_core::{
    BorderRadius, Color, DrawCommand, FillRule, Gradient, GradientKind, GradientStop,
    GradientStops, ImageData, ImageFilter, LineCap, LineJoin, Paint, PathData, PathStyle, PathVerb,
    RectStyle, RendererError, Scale, Shadow, ShapeStyle, Stroke, TextStyle,
};
pub use services_core::AppPathsProvider;
#[cfg(feature = "di")]
pub use services_core::{Scope, provide, try_inject, with_service};
#[cfg(feature = "runtime")]
pub use theme_core::{Theme, WidgetTheme, set_theme_with_widgets, use_theme, use_widget_theme};
#[cfg(all(feature = "runtime", feature = "svg"))]
pub use ui_core::Svg;
#[cfg(feature = "runtime")]
pub use ui_core::{
    Button, ButtonStyle, Canvas, ClippedItem, Component, ComponentList, Container, EventResult,
    Image, LayoutItem, LayoutScrollArea, Line, NodeId, NodeVec, Path, Rectangle, RenderNode,
    ScrollbarStyle, Slots, StyledContainer, Text, WidgetCtx, box_item, compute_layout, mark_dirty,
    new_container, new_leaf, set_display, track_layout,
};

#[cfg(all(feature = "preview", not(target_os = "android")))]
mod preview;
#[cfg(all(feature = "preview", not(target_os = "android")))]
pub use preview::run_preview_window;

#[cfg(feature = "dev")]
pub use hot_state::{hot_restore_json, hot_signal, hot_snapshot_json, probe};
#[cfg(all(feature = "runtime", target_os = "android"))]
pub use platform_android::AndroidApp;
#[cfg(all(feature = "runtime", target_os = "android"))]
pub use runner::run_android_app_with_name;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::run_app_with_name;
#[cfg(all(feature = "dev", not(target_os = "android")))]
pub use runner::run_hot_reload_host;

pub use rsx_macros::app;

#[cfg(all(feature = "dev", feature = "preview", not(target_os = "android")))]
pub use preview_runner::make_hot_preview_app;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use preview_runner::{try_run_preview, try_run_test};
