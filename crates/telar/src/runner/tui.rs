//! Starting an app on the terminal backend.

use std::sync::Arc;

use platform_tui::{OscClipboard, TuiPlatform, TuiPlatformConfig};
use renderer_tui::{CellMetrics, CellSize, ColorDepth, TuiConfig, TuiRendererFactory};
use services_core::{AppPathsProvider, SystemPaths};

use crate::app::App;
use crate::app_config::AppConfig;

/// How a terminal app is set up. Everything here is a property of the *terminal*, not of the application, which is why none of it lives in [`AppConfig`].
#[derive(Clone, Debug)]
pub struct TuiOptions {
    /// How many logical pixels one character cell stands for. Raising it makes a layout authored for the desktop occupy fewer cells; the default is the proportion of a typical monospace face.
    pub cell: CellSize,
    /// `None` asks the terminal what it can do. Set it to override a terminal that under-reports.
    pub depth: Option<ColorDepth>,
    pub mouse: bool,
    /// Whether Ctrl+C asks the app to close. See `TuiPlatformConfig::quit_on_ctrl_c`.
    pub quit_on_ctrl_c: bool,
}

impl Default for TuiOptions {
    fn default() -> Self {
        Self {
            cell: CellSize::default(),
            depth: None,
            mouse: true,
            quit_on_ctrl_c: true,
        }
    }
}

/// Runs an app in the terminal it was launched from.
///
/// The one thing that has to happen before anything else is the measurer: layout asks how wide a string is while the tree is being built, and the answer here is a count of cells rather than a shaped advance. It is installed unconditionally, so it wins over the raster measurer any later font load would offer.
pub fn run_tui_app_with_name<A: App>(
    config: AppConfig,
    options: TuiOptions,
    app: A,
    app_name: &str,
) {
    renderer_core::set_text_metrics(CellMetrics::new(options.cell));
    // A cell is the smallest step this surface can show, so easing across a wheel notch would repaint the screen several times to draw the same rows.
    ui_tree::set_smooth_wheel(false);
    services_core::set_clipboard(Arc::new(OscClipboard::new()));

    let paths: Arc<dyn AppPathsProvider> = Arc::new(SystemPaths);
    let platform = TuiPlatform::new(TuiPlatformConfig {
        cell_width: options.cell.width,
        cell_height: options.cell.height,
        mouse: options.mouse,
        quit_on_ctrl_c: options.quit_on_ctrl_c,
    });
    let factory = TuiRendererFactory::new(TuiConfig {
        cell: options.cell,
        depth: options.depth.unwrap_or_else(ColorDepth::detect),
        ..TuiConfig::default()
    });

    if let Err(e) = super::run_with_platform_and_renderer::<_, _, A, ()>(
        platform, factory, config, paths, app, app_name,
    ) {
        // The terminal is restored by the platform's own teardown and by its panic hook, so by the time this prints, the message lands on a shell the user can read it in.
        eprintln!("telar: {e}");
        std::process::exit(1);
    }
}
