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
#[cfg(feature = "plugin")]
pub mod plugin;
#[cfg(feature = "runtime")]
pub mod runner;
#[cfg(feature = "runtime")]
pub mod surface;
#[cfg(feature = "runtime")]
pub mod window;

pub use config::RendererBackend;

#[cfg(feature = "runtime")]
mod preview_runner;

#[cfg(feature = "runtime")]
pub use preview_runner::PreviewEntry;

#[cfg(feature = "runtime")]
pub use app_config::AppConfig;
#[cfg(feature = "runtime")]
pub use app_context::{AppCtx, RedrawWaker};
#[cfg(feature = "runtime")]
pub use prefs::UserPrefs;
pub use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
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
#[cfg(feature = "plugin")]
pub use plugin::EmbeddedApp;
// Always-on, no feature gate (D2 in docs/animations.md): kernel functionality, not an opt-in module. The transpiler emits `motion::Animated`/`motion::tween`/`motion::spring`/`motion::Easing` paths against this re-export.
pub use motion_core as motion;
// Always-on for the same reason as `motion`: the transpiler emits `i18n::translate` paths and the baked
// `crate::__rsx_i18n` catalog module references `i18n::{Catalog, Message, Part, Entry}`, so the facade must
// always expose them. Inert unless the app has a `locales/` catalog — nothing is baked or linked otherwise.
pub use i18n_core as i18n;
pub use i18n_core::{current_locale, detect_system_locale, init_locale, set_locale, use_locale};
#[cfg(feature = "runtime")]
pub use platform_core::{
    Event, FullscreenMode, Key, NamedKey, ScrollDelta, WindowCommand, WindowConfig, WindowPosition,
    push_window_command, take_window_commands,
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
    RectStyle, RendererError, Scale, Shadow, ShapeStyle, Stroke, TextAlign, TextRun, TextStyle,
};
pub use services_core::AppPathsProvider;
// Always available now: `ui_core::Surface` composes the per-surface service scope, so the DI/context feature is
// wired into every GUI build (via ui-core → services-core/di), not opt-in.
pub use services_core::{Scope, provide, try_inject, with_service};
#[cfg(feature = "runtime")]
pub use surface::{
    SurfaceContent, SurfaceControl, SurfaceHost, SurfaceToken, has_surface_host, open_surface,
    set_surface_host,
};
#[cfg(feature = "runtime")]
pub use theme_core::{
    Theme, ThemeTokens, follow_system, init_mode, is_dark, register_mode, set_dark, set_light_dark,
    set_mode, set_system_dark, set_theme, toggle_dark, use_mode, use_theme, use_theme_tokens,
};
#[cfg(all(feature = "runtime", feature = "svg"))]
pub use ui_core::Svg;
#[cfg(feature = "async-assets")]
pub use ui_core::{AssetSource, AssetState};
#[cfg(feature = "runtime")]
pub use ui_core::{
    Canvas, ChildSlot, ClippedItem, Component, ComponentList, Container, DEFAULT_SCRIM,
    EventResult, Image, Input, LayoutItem, LayoutScrollArea, Line, LineGutter, NodeId, NodeVec,
    Overlay, Path, ReactiveList, Rectangle, RenderNode, RichText, ScrollViewport, ScrollbarStyle,
    Slots, StyledContainer, SurfaceAlign, SurfaceAnchor, SurfaceFrameStyle, SurfacePlacement,
    SurfaceRole, SurfaceRoot, SurfaceScaffold, SurfaceSize, Text, TextArea, anchor_rect, box_item,
    box_transform, compute_layout, focus, fragment, fragment_gap, fragment_positional,
    fragment_positional_gap, interactive_rects, mark_dirty, new_container, new_leaf,
    relayout_if_dirty, remove_node, reset_layout_runtime, set_children, set_display,
    set_min_height, set_overlay_host, surface_frame, track_layout,
};

#[cfg(feature = "navigate")]
pub use navigate_core::{NavHost, NavPage, NavTransition, Navigator, SimplePage};

#[cfg(feature = "components")]
pub use ui_components::{
    AccordionProps, BadgeProps, ButtonProps, CheckboxProps, ChipProps, DrawerProps, HeadingProps,
    MenuProps, ModalProps, ProgressProps, RadioProps, SectionProps, SelectProps, SliderProps,
    SpinnerProps, StepperProps, TabsProps, TextFieldProps, ToggleProps, TooltipProps, accordion,
    badge, button, checkbox, chip, drawer, heading, hover_reveal, menu, modal, progress, radio,
    section, select, slider, spinner, stepper, tabs, text_field, toggle, tooltip,
};

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
pub use runner::build_surface_handler;
#[cfg(all(feature = "runtime", target_os = "android"))]
pub use runner::run_android_app_with_name;
#[cfg(all(feature = "dev", not(target_os = "android")))]
pub use runner::run_hot_reload_host;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::run_multi_with_platform;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::run_with_platform;
pub use runner::set_default_font_family;
#[cfg(all(feature = "runtime", feature = "desktop", not(target_os = "android")))]
pub use runner::{open_window, run_app_windowed, run_app_with_name, run_multi_app_with_name};

pub use rsx_macros::{app, rsx_modules, t};

#[cfg(all(feature = "dev", feature = "preview", not(target_os = "android")))]
pub use preview_runner::make_hot_preview_app;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use preview_runner::{try_run_preview, try_run_test};
