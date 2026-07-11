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
#[cfg(feature = "runtime")]
pub use platform_core::{
    Event, FullscreenMode, Key, NamedKey, ScrollDelta, WindowConfig, WindowPosition,
};
// Backend-author API: an out-of-tree `Platform` (e.g. a Wayland layer-shell backend) implements `Platform`
// and `Window` against these, driving a full rsx app via `run_with_platform` without depending on
// `platform-core` directly.
#[cfg(feature = "runtime")]
pub use platform_core::{
    EventHandler, ModifiersState, MultiSurfacePlatform, Platform, PlatformError, PointerButton,
    PointerSource, SurfaceId, Window,
};
#[cfg(all(feature = "runtime", feature = "desktop", not(target_os = "android")))]
pub use platform_desktop::DesktopPathsProvider;
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
    RectStyle, RendererError, Scale, Shadow, ShapeStyle, Stroke, TextAlign, TextStyle,
};
pub use services_core::AppPathsProvider;
#[cfg(feature = "di")]
pub use services_core::{Scope, provide, try_inject, with_service};
#[cfg(feature = "runtime")]
pub use theme_core::{
    Theme, ThemeTokens, follow_system, init_mode, is_dark, register_mode, set_dark, set_light_dark,
    set_mode, set_system_dark, set_theme, toggle_dark, use_mode, use_theme, use_theme_tokens,
};
#[cfg(all(feature = "runtime", feature = "svg"))]
pub use ui_core::Svg;
#[cfg(feature = "runtime")]
pub use ui_core::{
    Canvas, ChildSlot, ClippedItem, Component, ComponentList, Container, EventResult, Image, Input,
    LayoutItem, LayoutScrollArea, Line, NodeId, NodeVec, Overlay, Path, ReactiveList, Rectangle,
    RenderNode, ScrollbarStyle, Slots, StyledContainer, Text, box_item, box_transform,
    compute_layout, focus, fragment, fragment_gap, fragment_positional, fragment_positional_gap,
    mark_dirty, new_container, new_leaf, relayout_if_dirty, reset_layout_runtime, set_display,
    set_overlay_host, track_layout,
};

// Opt-in component catalogue. Re-exported at the prelude root so generated component calls resolve them
// (`button`/`ButtonProps`/…) by bare name through the `use rsx::*` every transpiled file emits.
#[cfg(feature = "components")]
pub use ui_components::{
    AccordionProps, BadgeProps, ButtonProps, CheckboxProps, ChipProps, DrawerProps, HeadingProps,
    MenuProps, ModalProps, ProgressProps, RadioProps, SectionProps, SelectProps, SliderProps,
    SpinnerProps, StepperProps, TabsProps, TextFieldProps, ToggleProps, TooltipProps, accordion,
    badge, button, checkbox, chip, drawer, heading, menu, modal, progress, radio, section, select,
    slider, spinner, stepper, tabs, text_field, toggle, tooltip,
};

/// Consults this crate's overlay registry and reports whether an overlay consumed the pointer event. The
/// hot-reload dylib exports this (as `_rsx_hot_dispatch_overlays`) so the host can route modal events into
/// the dylib's registry across the FFI boundary; the runner calls it via [`App::dispatch_overlays`].
#[cfg(feature = "runtime")]
pub fn dispatch_overlays(event: &Event) -> bool {
    ui_core::dispatch_overlays(event) == EventResult::Handled
}

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
#[cfg(all(feature = "dev", not(target_os = "android")))]
pub use runner::run_hot_reload_host;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::run_multi_with_platform;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::run_with_platform;
#[cfg(all(feature = "runtime", feature = "desktop", not(target_os = "android")))]
pub use runner::{run_app_with_name, run_multi_app_with_name};

pub use rsx_macros::{app, rsx_modules};

#[cfg(all(feature = "dev", feature = "preview", not(target_os = "android")))]
pub use preview_runner::make_hot_preview_app;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use preview_runner::{try_run_preview, try_run_test};
