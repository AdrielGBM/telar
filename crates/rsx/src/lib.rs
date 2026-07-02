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
#[derive(Clone)]
pub struct PreviewEntry {
    pub component_name: &'static str,
    pub preview_name: &'static str,
    pub build: fn(&mut WidgetCtx) -> Result<Box<dyn LayoutItem>, LayoutError>,
}

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
pub use geometry_core::{Point, Rect, Transform};
#[cfg(feature = "runtime")]
pub use layout_core::{
    AlignItems, AvailableSpace, JustifyContent, LayoutError, LayoutStyle, SizeDimension,
    TemplateTrack,
};
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub use paths::DesktopPathsProvider;
#[cfg(feature = "runtime")]
pub use platform_core::{Event, FullscreenMode, ScrollDelta, WindowConfig, WindowPosition};
#[cfg(feature = "runtime")]
pub use reactive_core::{
    Effect, Memo, ReadSignal, RwSignal, batch, effect, memo, reset_runtime, signal,
};
#[cfg(feature = "runtime")]
pub use renderer_core::{
    BorderRadius, Color, DrawCommand, FillRule, Gradient, GradientKind, GradientStop,
    GradientStops, ImageData, ImageFilter, LineCap, LineJoin, Paint, PathData, PathStyle, PathVerb,
    RectStyle, RendererError, Scale, Shadow, ShapeStyle, Stroke, TextStyle,
};
#[cfg(all(feature = "runtime", feature = "svg"))]
pub use renderer_core::{SvgData, SvgError};
pub use services_core::AppPathsProvider;
#[cfg(feature = "di")]
pub use services_core::{Scope, provide, try_inject, with_service};
#[cfg(feature = "runtime")]
pub use theme_core::{Theme, WidgetTheme, set_theme_with_widgets, use_theme, use_widget_theme};
#[cfg(all(feature = "runtime", feature = "svg"))]
pub use ui_core::Svg;
#[cfg(feature = "runtime")]
pub use ui_core::{
    Button, ButtonStyle, Canvas, Component, ComponentList, Container, EventResult, Image,
    LayoutItem, LayoutScrollArea, Line, NodeId, NodeVec, Path, Rectangle, RenderNode,
    ScrollbarStyle, StyledContainer, Text, WidgetCtx, box_item, compute_layout, mark_dirty,
    new_container, new_leaf, track_layout,
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
pub fn make_hot_preview_app(entries: Vec<PreviewEntry>) -> Box<dyn App> {
    Box::new(preview::PreviewApp { entries })
}

#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub fn try_run_preview(entries: Vec<PreviewEntry>, config: AppConfig) -> bool {
    #[cfg(feature = "preview")]
    {
        run_preview_window(entries, config);
        return true;
    }
    #[allow(unreachable_code)]
    let _ = (entries, config);
    false
}

/// Renders every preview component headlessly (build → layout → flatten) and exits with a non-zero code if any panics or returns a layout error. Backs `cargo rsx test`, entered via the `RSX_TEST` env var set on the app binary.
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub fn try_run_test(entries: Vec<PreviewEntry>, config: AppConfig) -> ! {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let width = config.window.width as f32;
    let height = config.window.height as f32;
    println!("running {} preview component(s)", entries.len());

    let mut passed = 0usize;
    let mut failed = 0usize;
    for entry in &entries {
        let label = format!("{}::{}", entry.component_name, entry.preview_name);
        // Do NOT reset the runtime between components: the app's setup block installed the theme once, and resetting would drop it, making previews that read theme tokens panic spuriously.
        let outcome = catch_unwind(AssertUnwindSafe(|| -> Result<usize, LayoutError> {
            let mut ctx = WidgetCtx::new();
            let item = (entry.build)(&mut ctx)?;
            let node = item.layout_node();
            compute_layout(
                &mut ctx,
                node,
                AvailableSpace::Definite(width),
                AvailableSpace::Definite(height),
            )?;
            let tree = ComponentList::new(item);
            Ok(tree.commands().len())
        }));
        match outcome {
            Ok(Ok(count)) => {
                passed += 1;
                println!("  ok    {label}  ({count} draw commands)");
            }
            Ok(Err(err)) => {
                failed += 1;
                println!("  FAIL  {label}  layout error: {err}");
            }
            Err(_) => {
                failed += 1;
                println!("  FAIL  {label}  panicked during render");
            }
        }
    }

    println!();
    println!("test result: {passed} passed, {failed} failed");
    std::process::exit(if failed == 0 { 0 } else { 1 });
}

#[macro_export]
macro_rules! children {
    ($($item:expr),* $(,)?) => {
        vec![$($crate::box_item($item)),*]
    }
}

/// Caches an `Arc<str>` per call site in thread-local storage so a string literal allocates at most once per thread instead of once per frame.
#[macro_export]
macro_rules! static_rc_str {
    ($s:literal) => {{
        thread_local! {
            static V: ::std::sync::Arc<str> = ::std::sync::Arc::from($s as &str);
        }
        V.with(::std::sync::Arc::clone)
    }};
}
