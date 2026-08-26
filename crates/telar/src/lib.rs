mod macros;

pub mod config;

#[cfg(feature = "runtime")]
pub mod app;
#[cfg(feature = "runtime")]
pub mod app_config;
#[cfg(feature = "runtime")]
pub mod app_context;
#[cfg(feature = "http-assets")]
pub mod async_assets;
#[cfg(feature = "runtime")]
pub mod dev_plugin;
#[cfg(feature = "dev")]
pub mod dev_tools;
#[cfg(feature = "runtime")]
mod direction;
#[cfg(any(
    all(feature = "dev", not(target_os = "android")),
    feature = "plugin-host"
))]
mod dylib;
#[cfg(feature = "runtime")]
pub mod files;
#[cfg(all(feature = "dev", not(target_os = "android")))]
pub mod hot;
#[cfg(feature = "dev")]
pub mod hot_state;
#[cfg(feature = "plugin")]
pub mod plugin;
#[cfg(feature = "runtime")]
pub mod prefs;
#[cfg(feature = "platform-headless")]
mod raster;
#[cfg(feature = "runtime")]
pub mod runner;
#[cfg(feature = "runtime")]
pub mod surface;
#[cfg(all(feature = "runtime", feature = "testing"))]
pub mod testing;
#[cfg(feature = "hardware")]
mod texture_ui;
#[cfg(feature = "runtime")]
pub mod tree;
#[cfg(feature = "watch")]
pub mod watch;
#[cfg(feature = "runtime")]
pub mod window;

pub use config::RendererBackend;

#[cfg(feature = "runtime")]
mod preview_runner;

#[cfg(feature = "runtime")]
pub use preview_runner::{PreviewEntry, PreviewSurface};

#[cfg(feature = "runtime")]
pub use app::App;
#[cfg(feature = "runtime")]
pub use app_config::AppConfig;
#[cfg(feature = "runtime")]
pub use app_context::{AppCtx, RedrawWaker};
#[cfg(feature = "runtime")]
pub use dev_plugin::{DevAction, DevPlugin};
#[cfg(feature = "runtime")]
pub use geometry_core::{ObjectFit, Point, Rect, Transform};
#[cfg(feature = "runtime")]
pub use layout_core::{
    AlignItems, AvailableSpace, Direction, JustifyContent, LayoutError, LayoutStyle, Margin,
    SizeDimension, TemplateTrack,
};
#[cfg(feature = "plugin")]
pub use plugin::EmbeddedApp;
#[cfg(feature = "runtime")]
pub use prefs::UserPrefs;
pub use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
#[cfg(feature = "runtime")]
pub use tree::{Frame, HotTree, LocalTree, UiTree};
// Named in the tree shims the `app!` macro exports, so it has to be reachable through the facade.
#[cfg(feature = "runtime")]
pub use ui_tree::SegmentNodeInfo;
// Always-on, no feature gate (D2 in docs/animations.md): kernel functionality, not an opt-in module. The transpiler emits `motion::Animated`/`motion::tween`/`motion::spring`/`motion::Easing` paths against this re-export.
pub use motion_core as motion;
// Always-on for the same reason as `motion`: the transpiler emits `i18n::translate` paths and the baked
// `crate::__rsx_i18n` catalog module references `i18n::{Catalog, Message, Part, Entry}`, so the facade must
// always expose them. Inert unless the app has a `locales/` catalog — nothing is baked or linked otherwise.
#[cfg(feature = "runtime")]
pub use direction::follow_locale_direction;
pub use i18n_core as i18n;
// `set_catalog` is app lifecycle, like `set_locale`, so it belongs at the root. Its lookup — `i18n::t` — is
// deliberately not re-exported here: `t` at the facade root is already the `t!` macro, and a second `t` that
// resolves at runtime rather than at expansion is a name nobody could read at a glance.
pub use i18n_core::set_catalog;
pub use i18n_core::{current_locale, detect_system_locale, set_locale, use_locale};
#[cfg(feature = "runtime")]
pub use platform_core::{
    Cursor, Event, FullscreenMode, Key, NamedKey, ScrollDelta, WindowCommand, WindowConfig,
    WindowPosition, push_window_command, take_window_commands,
};
#[cfg(feature = "watch")]
pub use watch::watch_path;
// The seam for an application that renders its own GPU content: it borrows the device Telar draws with, and
// re-exports the `wgpu` both sides must agree on. **Depend on `telar::gpu::wgpu`, not on `wgpu` directly**:
// two `wgpu` versions in one binary are two incompatible `Device` types, and the error names neither crate.
#[cfg(feature = "hardware")]
pub use renderer_hardware::gpu;
// The same seam facing the other way: Telar composing into a texture the application owns.
#[cfg(feature = "hardware")]
pub use texture_ui::{TextureUi, TextureUiError};
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
    Effect, Emitter, Memo, OwnerGuard, OwnerId, ReadSignal, RwSignal, Source, Task, batch,
    begin_batch, current_owner, derive, derive_pair, detached, dispose_owner, drain_tasks, effect,
    end_batch, memo, on_cleanup, owner_scope, reset_runtime, reset_tasks, set_task_waker, signal,
    spawn_stream, spawn_task, with_owner,
};
#[cfg(all(feature = "runtime", feature = "dynamic-image"))]
pub use renderer_assets::{ImageError, decode};
#[cfg(all(feature = "runtime", feature = "svg"))]
pub use renderer_assets::{SvgData, SvgError, VectorCommand};
#[cfg(all(feature = "runtime", feature = "dynamic-svg"))]
pub use renderer_assets::{static_key, svg_cached};
#[cfg(feature = "runtime")]
pub use renderer_core::{
    Border, BorderRadius, Clamp, Color, Declared, DrawCommand, DrawState, FillRule, FontFamily,
    FontStyle, Gradient, GradientKind, GradientStop, GradientStops, ImageData, LineCap, LineHeight,
    LineJoin, Paint, PathData, PathStyle, PathVerb, Raster, RectStyle, RendererError, Scale,
    Shadow, ShapeStyle, Span, Stroke, TextAlign, TextShadow, TextStyle, TextWrap,
    for_each_with_matrix, hash_draw_commands, transform_clip_rect,
};
// Backend-author API, the drawing half: a frontend implements `RendererFactory`, installs it with `run_with_platform_and_renderer`, and installs `TextMetrics` for whatever "how wide is this string" means on its surface.
#[cfg(feature = "runtime")]
pub use renderer_core::{
    FontConfig, RenderBackend, RendererBuild, RendererFactory, TextMetrics,
    set_default_text_metrics, set_text_metrics,
};

/// Whether `family` names a font installed on this system.
///
/// Both [`AppConfig::font_family`](crate::AppConfig::font_family) and
/// [`TextStyle::with_font_family`](crate::TextStyle::with_font_family) take any name and fall back silently
/// when the family is not installed, so this is how an application warns instead. Answered by the database
/// the text shaper already loaded — asking it costs nothing, where a second `fontdb` is a full system font
/// scan and a second answer that can disagree with the one the text is shaped in.
#[cfg(feature = "runtime")]
pub fn font_family_available(family: &str) -> bool {
    renderer_text::font_family_available(family)
}

/// Installs the glyph-shaping text measurer, for code that lays out text with no runner behind it — a layout test,
/// or a tool that composes a tree only to measure it.
///
/// An app never needs this: the runner installs it on resume with the app's own fonts. Nothing happens if a measurer
/// is already installed.
#[cfg(feature = "runtime")]
pub fn install_default_text_metrics() {
    renderer_core::set_default_text_metrics(renderer_text::ShaperMetrics);
}

/// What the CPU renderer's caches are holding, and a way to make them let go. Exposed so an app can answer "is the
/// memory in the renderer?" from outside the renderer, which nothing short of a heap profiler could do before.
#[cfg(feature = "software")]
pub use renderer_software::{CacheStat, cache_stats, sweep_idle as sweep_renderer_caches};
pub use services_core::app_paths as paths;
pub use services_core::{AppPathsProvider, NoPaths};
pub use services_core::{Clipboard, clipboard, clipboard_text, set_clipboard, set_clipboard_text};
// Available in every GUI build, not opt-in: `ui_core::Surface` composes the per-surface service scope, so
// `runtime` pulls in ui-core which turns on services-core/di. A non-GUI build (the `cargo-telar` tool depends on
// `rsx` with default-features off) has no ui-core, hence no `di`, hence nothing to re-export.
#[cfg(feature = "http-assets")]
pub use async_assets::HttpAssetSource;
#[cfg(feature = "runtime")]
pub use services_core::{Scope, context, provide, set_context, try_inject, with_service};
#[cfg(feature = "runtime")]
pub use surface::{
    SurfaceContent, SurfaceControl, SurfaceHost, SurfaceToken, open_surface, set_surface_host,
    surface_content,
};
#[cfg(feature = "runtime")]
pub use theme_core::{
    ControlSize, ThemeTokens, active_mode, control_scale, follow_system, register_mode,
    set_control_size, set_mode, set_system_dark, set_theme, use_control_size, use_theme,
    use_theme_tokens,
};
#[cfg(all(feature = "runtime", feature = "svg"))]
pub use ui_core::Svg;
#[cfg(feature = "async-assets")]
pub use ui_core::{AssetSource, AssetState};
#[cfg(feature = "runtime")]
pub use ui_core::{
    Axis, Canvas, ChildSlot, Children, ClipAxis, ClippedItem, Component, ComponentList, Container,
    DEFAULT_SCRIM, DragStart, Edge, EventResult, Image, Inherited, Input, KeyNav, KeyNavMove,
    LayoutItem, LayoutScrollArea, Lazy, LineGutter, NodeId, NodeVec, Overlay, Path, PointerButtons,
    ReactiveList, Rectangle, RenderNode, ScrollPage, ScrollViewport, ScrollbarStyle, Slots,
    StyledContainer, SurfaceScaffold, SurfaceTransition, Text, TextArea, VirtualList, WindowRoot,
    anchor_rect, apply_move, box_item, box_transform, close_overlay, compute_layout,
    current_direction, declare, dismiss_depth, dismiss_top, drag_start, drag_travel, focus,
    fragment, fragment_positional, inherited_text_style, insertion_index, interactive_rects, kept,
    key_held, key_nav_apply, key_nav_apply_grid, key_pressed, logical_border_radius,
    logical_border_widths, mark_dirty, modifiers, new_container, new_leaf, observe_keyboard,
    observe_pointer, open_overlay, overlay_state, pointer_buttons, relayout_if_dirty, remove_node,
    set_children, set_direction, set_display, set_min_height, set_overlay_host, track_layout,
    transform_pointer, undeclare, use_context, use_direction, use_dismiss_depth, visible_window,
};

/// Empties the layout runtime for a fresh tree, and installs the glyph measurer if nothing installed one.
///
/// The measurer rides along because sizing text is part of laying it out, and this is the first call every tree
/// makes: a tree built with no runner behind it — a layout test, a tool measuring a page — would otherwise have to
/// ask for one separately. A frontend that installed its own keeps it; see [`install_default_text_metrics`].
#[cfg(feature = "runtime")]
pub fn reset_layout_runtime() {
    install_default_text_metrics();
    ui_core::reset_layout_runtime();
}

#[cfg(feature = "navigate")]
pub use navigate_core::{
    NavHost, NavPage, NavTransition, Navigator, PagePolicy, SimplePage, TabHost, TabStacks,
};

#[cfg(feature = "components")]
pub use ui_components::{
    AccordionProps, BadgeProps, ButtonProps, CheckboxProps, ChipProps, DrawerProps, GroupProps,
    HeadingProps, ItemProps, MIN_FRAME_SIZE, MenuProps, ModalProps, ProgressProps, RadioProps,
    ReorderableProps, SectionProps, SelectProps, SliderProps, SpinnerProps, StepperProps,
    SurfaceFrameStyle, TabsProps, TextFieldProps, ToggleProps, TooltipProps, WindowControls,
    accordion, badge, button, checkbox, chip, drawer, group, heading, item, menu, modal, progress,
    radio, reorderable, section, select, separator, slider, spinner, stepper, tabs, text_field,
    toggle, tooltip, window_frame,
};

#[cfg(feature = "runtime")]
pub fn dispatch_overlays(event: &Event) -> bool {
    ui_core::dispatch_overlays(event) == EventResult::Handled
}

#[cfg(all(
    any(feature = "preview", feature = "preview-headless"),
    not(target_os = "android")
))]
mod preview;
#[cfg(all(
    any(feature = "preview", feature = "preview-headless"),
    not(target_os = "android")
))]
pub use preview::PreviewApp;
#[cfg(all(feature = "preview-headless", not(target_os = "android")))]
pub use preview::run_preview_png;
#[cfg(feature = "platform-headless")]
pub use raster::rasterize;

#[cfg(feature = "dev")]
pub use hot_state::{hot_restore_json, hot_signal, hot_snapshot_json, probe};

/// Without `dev` there is no dylib swap to survive, so the key is inert and this degrades to a plain signal.
/// The bounds match the `dev` build's so a type that compiles here cannot fail once hot-reload is on — letting
/// hand-written app state (a navigation stack, an active locale) be declared once instead of behind a `cfg`.
#[cfg(all(feature = "runtime", not(feature = "dev")))]
pub fn hot_signal<T>(key: &str, init: T) -> reactive_core::RwSignal<T>
where
    T: Clone + serde::Serialize + serde::de::DeserializeOwned + 'static,
{
    let _ = key;
    reactive_core::signal(init)
}
#[cfg(all(feature = "runtime", target_os = "android"))]
pub use platform_android::AndroidApp;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::build_surface_handler;
#[cfg(all(feature = "runtime", target_os = "android"))]
pub use runner::run_android_app_with_name;
#[cfg(all(feature = "dev", not(target_os = "android")))]
pub use runner::run_hot_reload_host;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::run_multi_with_platform;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::run_with_platform;
#[cfg(feature = "runtime")]
// The renderer seam, and what a window has to be for the built-in renderers to draw on it.
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::{SurfaceWindow, run_with_platform_and_renderer};
#[cfg(all(feature = "runtime", feature = "desktop", not(target_os = "android")))]
pub use runner::{open_window, run_app_windowed, run_app_with_name};

pub use telar_macros::{ThemeTokens, app, rsx_modules, t};

#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use preview_runner::{dev_entry, try_run_test};
