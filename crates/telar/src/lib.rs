//! The Telar facade: the crate every application depends on.
//!
//! Re-exports the reactive primitives, the widget kernel and the geometry types, and carries the runner that turns a mounted tree into a window. The widget catalogue and the page stack are not here: they are the `telar-components` and `telar-navigate` plugins, which an application adds beside this crate.
//!
//! # Feature flags
//!
//! Enable one target and stop. The list below is long because nothing is bundled, not because a build
//! carries it: a terminal app links about 160 crates where a desktop one links about 410.
#![cfg_attr(
    feature = "document-features",
    doc = document_features::document_features!()
)]
#![warn(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

mod macros;

pub mod config;

mod licenses;

#[cfg(feature = "runtime")]
pub mod app;
#[cfg(feature = "runtime")]
pub mod app_config;
#[cfg(feature = "runtime")]
pub mod app_runtime;
#[cfg(feature = "runtime")]
mod direction;
#[cfg(feature = "runtime")]
pub mod files;
#[cfg(all(
    feature = "dev",
    not(target_os = "android"),
    not(target_arch = "wasm32")
))]
pub mod hot;
#[cfg(any(
    feature = "dev",
    feature = "prerender",
    all(feature = "web-dom", target_arch = "wasm32")
))]
pub mod hot_state;
#[cfg(feature = "runtime")]
mod location;
#[cfg(feature = "runtime")]
mod location_locale;
#[cfg(feature = "runtime")]
pub mod prefs;
#[cfg(feature = "headless")]
mod raster;
#[cfg(feature = "runtime")]
pub mod runner;
#[cfg(feature = "runtime")]
pub mod surface;
#[cfg(feature = "runtime")]
mod system_locale;
#[cfg(all(feature = "runtime", feature = "testing"))]
pub mod testing;
#[cfg(feature = "hardware")]
mod texture_ui;
#[cfg(feature = "runtime")]
pub mod tree;
#[cfg(feature = "runtime")]
pub mod user_preferences;
#[cfg(feature = "runtime")]
pub mod window;

pub use config::RendererBackend;

#[cfg(feature = "previews")]
pub mod preview;

/// Keeps items that exist only for previews — preview tables, props metadata, preview-only builder methods — in a crate's source while compiling them only when `telar/previews` is on, so a dependency carries its previews to an app that asks for them and costs every other build nothing.
#[cfg(feature = "previews")]
#[doc(hidden)]
#[macro_export]
macro_rules! __previews {
    ($($item:tt)*) => {
        $($item)*
    };
}

#[cfg(not(feature = "previews"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __previews {
    ($($item:tt)*) => {};
}

#[cfg(test)]
#[path = "previews_gate_test.rs"]
mod previews_gate_tests;

#[cfg(feature = "runtime")]
pub use app::App;
#[cfg(feature = "runtime")]
pub use app_config::AppConfig;
#[cfg(feature = "runtime")]
pub use platform_core::{AccessNode, AppCtx, RedrawWaker};
// For a backend author driving a handler by hand. An application implements `App` and names neither.
#[cfg(feature = "runtime")]
pub use app_runtime::{AppRuntime, LocalApp};
#[cfg(feature = "runtime")]
pub use geometry_core::{Insets, ObjectFit, Point, Rect, Size, Transform};
#[cfg(feature = "runtime")]
pub use layout_core::{
    AlignItems, AvailableSpace, Direction, JustifyContent, LayoutError, LayoutStyle, Margin,
    MeasureInput, SizeDimension, TemplateTrack,
};
#[cfg(feature = "runtime")]
pub use prefs::UserPrefs;
pub use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
// The faces a surface loads and the family it shapes in — what an `AppConfig` carries, as one value, for the seams that take it directly.
#[cfg(feature = "runtime")]
pub use runner::font_config::FontSetup;
#[cfg(feature = "runtime")]
pub use tree::{Frame, HotTree, LocalTree, UiTree};
#[cfg(feature = "runtime")]
pub use ui_tree::{DevAction, DevOverlay, OverlayResponse};
// Named in the tree shims the `app!` macro exports, so it has to be reachable through the facade.
#[cfg(feature = "runtime")]
pub use ui_tree::SegmentNodeInfo;
// Always on: kernel functionality, not an opt-in module. The transpiler emits `motion::` paths against this.
pub use motion_core as motion;
// Always on for the same reason: the transpiler emits `i18n::` paths and the baked catalog module names these types. Inert unless the app has a `locales/` catalog.
#[cfg(feature = "runtime")]
pub use direction::follow_locale_direction;
pub use i18n_core as i18n;
// App lifecycle, like `set_locale`, so it belongs at the root. Its lookup `i18n::t` is deliberately not re-exported: `t` here is already the `t!` macro, and a second `t` resolving at runtime would be unreadable.
pub use i18n_core::set_catalog;
// Named by the load-time constructor `app!` and `rsx_modules!` emit to install an application's baked catalog, so an application needs no `ctor` dependency of its own.
#[doc(hidden)]
pub use ctor as __ctor;
// The notice of the icons an application baked, installed by the same constructor and read back by `telar-icons`.
pub use i18n_core::{current_locale, negotiate_locale, set_locale, use_locale};
pub use licenses::icon_licenses;
#[doc(hidden)]
pub use licenses::install_icon_licenses as __install_icon_licenses;
#[cfg(feature = "runtime")]
pub use platform_core::{
    ConsumedKeys, Cursor, Event, FullscreenMode, Key, NamedKey, NumericValue, Orientation,
    ScrollDelta, WindowCommand, WindowConfig, WindowPosition, push_window_command,
    take_window_commands,
};
// The seam for an application rendering its own GPU content: it borrows the device Telar draws with, and re-exports the `wgpu` both sides must agree on. Two `wgpu` versions in one binary are two incompatible `Device` types, and the error names neither crate.
#[cfg(feature = "hardware")]
pub use renderer_hardware::gpu;
// The same seam facing the other way: Telar composing into a texture the application owns.
#[cfg(feature = "hardware")]
pub use texture_ui::{TextureUi, TextureUiError};
// Backend-author API: an out-of-tree `Platform` implements these and drives a full app through `run_with_platform` without depending on `platform-core` directly.
#[cfg(feature = "runtime")]
pub use platform_core::{
    EventHandler, KeyPairing, ModifiersState, MultiSurfacePlatform, Platform, PlatformError,
    PointerButton, PointerSource, SurfaceId, Window,
};
#[cfg(all(
    feature = "runtime",
    feature = "desktop-bare",
    not(target_os = "android"),
    not(target_arch = "wasm32")
))]
pub use platform_desktop::DesktopPathsProvider;
#[cfg(feature = "runtime")]
pub use reactive_core::{
    Effect, Emitter, Memo, OwnerGuard, OwnerId, Reactive, ReadSignal, RwSignal, Source, Task,
    Transaction, TransactionError, batch, begin_batch, current_owner, derive, derive_pair,
    detached, dispose_owner, drain_tasks, effect, end_batch, memo, on_cleanup, owner_scope,
    reset_runtime, reset_tasks, set_task_waker, signal, spawn_stream, spawn_task, with_owner,
};
#[cfg(all(feature = "runtime", feature = "svg"))]
pub use renderer_assets::{SvgData, SvgError, VectorCommand};
#[cfg(feature = "runtime")]
pub use renderer_core::{
    BlendMode, Border, BorderRadius, Clamp, Color, Dash, Declared, DecorationLength,
    DecorationLine, DecorationMetric, DrawCommand, DrawState, FillRule, FitWidth, FontFamily,
    FontFeatures, FontFit, FontStyle, FontTag, FontVariations, Gradient, GradientKind,
    GradientStop, GradientStops, ImageData, ImageFill, ImageSlice, LayerMask, LineCap, LineHeight,
    LineJoin, Paint, PathData, PathStyle, PathVerb, Raster, RectStyle, RendererError, Role, Scale,
    Semantics, Shadow, ShapeStyle, Span, Stroke, TextAlign, TextCase, TextDecoration, TextLength,
    TextShadow, TextStyle, TextWrap, fitted_font_size, for_each_with_matrix, hash_draw_commands,
    measure_text, text_metrics_generation, transform_clip_rect,
};
// The drawing half of the backend-author API: a frontend implements `RendererFactory` and installs a `TextMetrics` for whatever "how wide is this string" means on its surface.
#[cfg(feature = "runtime")]
pub use geometry_core::{LayoutGrid, layout_grid, set_layout_grid};
#[cfg(feature = "runtime")]
pub use renderer_core::{
    FontAsset, FontAxis, FontConfig, FontSource, FontWeight, RenderBackend, RendererBuild,
    RendererFactory, TextMetrics, set_default_text_metrics, set_text_metrics,
};

/// Whether this build turns font files into glyphs itself.
///
/// The question an application asks before embedding a face in [`AppConfig::fonts`](crate::AppConfig::fonts): a shaper on a target with no font directory behind it — a browser — finds nothing and measures every string to zero, so the app has to carry a face. A build that draws as a document has no shaper to feed. Its text is laid out and drawn by the browser in the browser's own fonts, and a face baked into the module would be bytes nothing reads.
///
/// A `const` rather than a function so the branch that answers it is folded away, and the faces behind an `include_bytes!` in the arm not taken never reach the binary.
pub const SHAPES_TEXT: bool = cfg!(feature = "shaper");

/// Whether `family` names a font installed on this system.
///
/// Both [`AppConfig::font_family`](crate::AppConfig::font_family) and [`TextStyle::with_font_family`](crate::TextStyle::with_font_family) take any name and fall back silently when the family is not installed, so this is how an application warns instead. Answered by the database the text shaper already loaded — asking it costs nothing, where a second `fontdb` is a full system font scan and a second answer that can disagree with the one the text is shaped in.
#[cfg(feature = "shaper")]
pub fn font_family_available(family: &str) -> bool {
    renderer_text::font_family_available(family)
}

/// Every font family text can be set in on this system, sorted and each once, for a list that offers them to [`set_font_family`] or [`TextStyle::with_font_family`](crate::TextStyle::with_font_family).
///
/// Read from the database the text shaper already loaded, like [`font_family_available`], and kept with it, so asking again copies a list rather than scanning, and every name in it is one text resolves. The faces an application ships ([`AppConfig::fonts`](crate::AppConfig::fonts)) are listed under the family they declare. A name starting with a dot is a face the platform keeps for itself and is left out.
///
/// The database only grows: a face added while the app runs is listed from then on. A list that should follow it reads [`use_text_metrics_generation`] beside this, which moves when one lands.
#[cfg(feature = "shaper")]
pub fn font_families() -> Vec<String> {
    renderer_text::font_families()
}

/// Installs the glyph-shaping text measurer, for code that lays out text with no runner behind it — a layout test, or a tool that composes a tree only to measure it.
///
/// An app never needs this: the runner installs it on resume with the app's own fonts. Nothing happens if a measurer is already installed.
#[cfg(feature = "shaper")]
pub fn install_default_text_metrics() {
    renderer_core::set_default_text_metrics(renderer_text::ShaperMetrics);
}

/// What the CPU renderer's caches are holding, and a way to make them let go. Exposed so an app can answer "is the memory in the renderer?" from outside the renderer, which nothing short of a heap profiler could do before.
#[cfg(feature = "software")]
pub use renderer_software::{CacheStat, cache_stats, sweep_idle as sweep_renderer_caches};
pub use services_core::app_paths as paths;
pub use services_core::{AppPathsProvider, NoPaths};
pub use services_core::{Clipboard, clipboard, clipboard_text, set_clipboard, set_clipboard_text};
pub use services_core::{
    FileStore, MemoryStore, PreferenceStore, set_preference_store, store_preference,
    stored_preference,
};
pub use services_core::{UriOpener, open_beside, open_uri, set_uri_opener};
// Available in every GUI build rather than opt-in: `ui_core::Surface` composes the per-surface service scope, so `runtime` turns on services-core/di. A non-GUI build has no ui-core and nothing to re-export.
#[cfg(feature = "runtime")]
pub use platform_core::{ColorScheme, SystemPreferences, system_locales_from_env};
// The app's address: always on, because every target has one to report even where no navigator follows it.
#[cfg(feature = "runtime")]
pub use location::navigate_back;
#[cfg(feature = "runtime")]
pub use location_locale::follow_location_locale;
#[cfg(feature = "runtime")]
pub use platform_core::{
    ArgumentLocation, FixedLocation, HistoryFollower, HistoryFollowerId, HistorySink, HistoryStep,
    HistoryUpdate, Location, LocationFormat, LocationSource, follow_location_history, history_back,
    is_following_location_history, location_format, location_history, location_locale,
    location_locales, location_pages, pages_of, push_anchor, push_location,
    receive_location_history, replace_location, report_location_history, rewrite_location_history,
    unfollow_location_history,
};
#[cfg(feature = "runtime")]
pub use platform_core::{
    CurrentKind, Destination, IntoDestination, Route, Uri, address_of, anchor, external, in_locale,
};
#[cfg(feature = "runtime")]
pub use preferences_core::{
    reduced_motion_override, set_reduced_motion_override, set_system_preferences,
    system_preferences, use_color_scheme, use_high_contrast, use_preferred_locales,
    use_reduced_motion, use_reduced_motion_override, use_system_preferences,
    use_system_reduced_motion,
};
#[cfg(feature = "runtime")]
pub use services_core::{Scope, context, provide, set_context, try_inject, with_service};
#[cfg(feature = "runtime")]
pub use surface::{
    SurfaceContent, SurfaceControl, SurfaceHost, SurfaceToken, open_surface, set_surface_host,
    surface_content,
};
#[cfg(feature = "runtime")]
pub use system_locale::follow_system_locale;
#[cfg(feature = "runtime")]
pub use theme_core::{
    ControlSize, ResolvedScheme, SchemePreference, ScopedTheme, Theme, ThemeExtensions,
    ThemeTokens, active_mode, control_scale, current_control_size, follow_system, is_dark,
    nearest_theme, register_mode, scheme_preference, set_control_size, set_mode,
    set_scheme_preference, set_theme, use_control_size, use_mode, use_resolved_scheme,
    use_scheme_preference, use_theme, use_theme_extension, use_theme_tokens,
};
#[cfg(all(feature = "runtime", feature = "svg"))]
pub use ui_core::Svg;
#[cfg(feature = "runtime")]
pub use ui_core::{
    AnchorRegistration, PageAnchor, ScrollLinked, ViewRange, enclosing_scroll_viewport, follow,
    has_anchor, range_progress, register_anchor, reveal_anchor, scroll_progress,
    scroll_progress_along, scroll_viewports_of, use_anchor_at, use_primary_scroll,
    use_scroll_viewport,
};
#[cfg(feature = "runtime")]
pub use ui_core::{
    Breakpoints, breakpoint, set_safe_area_insets, set_surface_size, surface_size,
    use_safe_area_insets, use_surface_height, use_surface_size, use_surface_width,
    use_text_metrics_generation,
};
#[cfg(feature = "runtime")]
pub use ui_core::{
    TitleParts, compose_title, open_surface_title, set_page_title, set_title_format, surface_title,
    use_surface_title,
};
// The family a surface's text shapes in where nothing above it names one: seeded from `AppConfig::font_family`, changed live with `set_font_family`.
#[cfg(feature = "runtime")]
pub use ui_core::{SurfaceCanvas, SurfaceFrame, composite_surface};
#[cfg(feature = "runtime")]
pub use ui_core::{SurfaceStyle, amend_surface};
#[cfg(feature = "runtime")]
pub use ui_core::{open_surface_font_family, set_font_family, use_font_family};
// The seam and nothing behind it: `telar-dynamic` carries the decoders and transports that plug in here, and an application's own plug in exactly the same way.
#[cfg(feature = "runtime")]
pub use ui_core::{
    Accessible, Axis, BuildFailure, COARSE_STEP, Canvas, Caret, ChildSlot, Children, Clip,
    ClipAxis, ClippedItem, Component, ComponentList, Container, DEFAULT_SCRIM, DismissRegistration,
    DragAxis, DragStart, Edge, ErrorBoundary, EventResult, FINE_STEP, FixedLayer, Image, Inherited,
    Input, IntoClip, KeyAnswer, KeyNav, KeyNavMove, LayoutItem, LayoutLeaf, LayoutScrollArea,
    LayoutTransition, Lazy, Mask, NodeId, NodeVec, Overlay, Path, Placement, PointerButtons,
    Presence, ReactiveList, Rectangle, RenderNode, ScrollPage, ScrollViewport, ScrollbarStyle,
    SlotRequest, Slots, StyledContainer, SurfaceScaffold, SurfaceTransition, Text, TextArea,
    TextRun, ThemeProvider, TransformOrigin, Transition, Underline, VirtualList, WindowRoot,
    absolute_rect, anchor_rect, animate_layout, apply_move, box_item, box_transform,
    box_transform_about, close_overlay, compute_layout, confirm_top, current_direction, declare,
    dismiss_depth, dismiss_top, drag_start, drag_travel, exits_in_flight, focus, follow_theme,
    fragment, fragment_positional, inherited_text_style, insertion_index, interactive_rects, kept,
    key_held, key_nav_apply, key_nav_apply_grid, key_pressed, line_box, logical_border_radius,
    logical_border_widths, mark_dirty, modifiers, new_container, new_leaf, observe_keyboard,
    observe_pointer, open_overlay, overlay_state, overlay_viewport, pointer_buttons, provide_theme,
    register_transaction, relayout_if_dirty, remove_node, requested_cursor, set_children,
    set_direction, set_display, set_min_height, set_overlay_host, single_line_box, step_factor,
    style_follows, track_layout, transform_pointer, undeclare, use_context, use_direction,
    use_dismiss_depth, visible_window,
};
#[cfg(feature = "async-assets")]
pub use ui_core::{
    AssetCache, AssetDecoder, AssetError, AssetKey, AssetLoader, AssetState, AssetTransport, Reply,
};

/// Empties the layout runtime for a fresh tree, and installs the glyph measurer if nothing installed one.
///
/// The measurer rides along because sizing text is part of laying it out, and this is the first call every tree makes: a tree built with no runner behind it — a layout test, a tool measuring a page — would otherwise have to ask for one separately. A frontend that installed its own keeps it; see [`install_default_text_metrics`].
///
/// A build with no shaper has no measurer to fall back to, and needs none: the only frontend that comes without one draws as a document, which measures with the engine that will draw the text and installs that before the first tree is built.
#[cfg(feature = "runtime")]
pub fn reset_layout_runtime() {
    #[cfg(feature = "shaper")]
    install_default_text_metrics();
    ui_core::reset_layout_runtime();
}

#[cfg(feature = "runtime")]
/// Offers an event to the overlay registry first, returning whether an overlay consumed it.
pub fn dispatch_overlays(event: &Event) -> bool {
    ui_core::dispatch_overlays(event) == EventResult::Handled
}

#[cfg(all(feature = "preview-headless", not(target_os = "android")))]
pub use preview::host::run_preview_png;
#[cfg(feature = "headless")]
pub use raster::rasterize;

#[cfg(any(
    feature = "dev",
    feature = "prerender",
    all(feature = "web-dom", target_arch = "wasm32")
))]
pub use hot_state::hot_signal;
#[cfg(any(feature = "dev", feature = "prerender"))]
pub use hot_state::{hot_restore_json, hot_snapshot_json, probe};

/// Without `dev` there is no dylib swap to survive, without `prerender` no page to carry it in, and without a page to take over none to read it back from, so the key is inert and this degrades to a plain signal. The bounds match the keyed build's so a type that compiles here cannot fail once either is on — letting hand-written app state (a navigation stack, an active locale) be declared once instead of behind a `cfg`.
#[cfg(all(
    feature = "runtime",
    not(any(
        feature = "dev",
        feature = "prerender",
        all(feature = "web-dom", target_arch = "wasm32")
    ))
))]
pub fn hot_signal<T>(key: &str, init: T) -> reactive_core::RwSignal<T>
where
    T: Clone + serde::Serialize + serde::de::DeserializeOwned + 'static,
{
    let _ = key;
    reactive_core::signal(init)
}
#[cfg(all(feature = "android-bare", target_os = "android"))]
pub use platform_android::AndroidApp;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::build_surface_handler;
#[cfg(all(feature = "prerender", not(target_arch = "wasm32")))]
pub use runner::prerender_page;
#[cfg(all(
    feature = "desktop-bare",
    feature = "dev",
    not(target_os = "android"),
    not(target_arch = "wasm32")
))]
pub use runner::run_hot_reload_host;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::run_multi_with_platform;
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::run_with_platform;
#[cfg(feature = "runtime")]
// The renderer seam, and what a window has to be for the built-in renderers to draw on it.
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::{SurfaceWindow, run_with_platform_and_renderer};
#[cfg(all(
    feature = "runtime",
    feature = "desktop-bare",
    not(target_os = "android"),
    not(target_arch = "wasm32")
))]
pub use runner::{open_window, run_app_windowed, run_desktop_app_with_name};
#[cfg(all(feature = "android-bare", target_os = "android"))]
pub use runner::{run_android_app_with_devtools, run_android_app_with_name};
// The frontend an app actually starts on. Not gated on `desktop`: choosing between the frontends a build has is the whole point of it, and a terminal-only build has no window to open.
#[cfg(all(feature = "runtime", feature = "tui", not(target_os = "android")))]
pub use runner::{TuiOptions, run_tui_app_with_name};
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use runner::{run_app_with_devtools, run_app_with_name};

// Always present, like `Props`: an enum derives it in every build, and the impl it expands to is gated behind `__previews!`.
pub use telar_macros::{PreviewArg, Props, ThemeTokens, app, component, rsx_modules, t};

#[cfg(all(feature = "previews", not(target_os = "android")))]
pub use preview::host::{dev_entry, try_run_test};
