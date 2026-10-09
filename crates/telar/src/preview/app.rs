//! The fallback preview host: every preview in one scrolling column, for a build with no workshop.

use crate::{
    App, BuildFailure, Color, Component, Container, LayoutError, LayoutItem, LayoutStyle,
    ScrollPage, Text, TextStyle, reset_layout_runtime,
};
#[cfg(feature = "preview-headless")]
use crate::{AppConfig, Size};

#[cfg(feature = "preview-headless")]
use super::host::{Args, fail_duplicate_ids, file_stem};
use super::host::{duplicate_ids, remounting, requested_preview};
#[cfg(feature = "preview-headless")]
use super::{Matrices, Play};
use super::{PreviewCtx, PreviewEntry};

/// An app that renders previews instead of its own root: the page `cargo telar preview` falls back to when the package declares no workshop.
///
/// Each preview is built inside its own [`crate::ErrorBoundary`], so one that fails or panics shows its error in place and the rest of the page keeps running, and is built again when an arg it read changes.
pub struct PreviewApp {
    entries: Vec<PreviewEntry>,
}

fn failure_label(failure: BuildFailure) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let message = format!("Error: {failure}");
    Ok(Box::new(Text::new(
        move || message.clone(),
        LayoutStyle::new(),
        || TextStyle::new(12.0, Color::rgba(0.9, 0.2, 0.2, 1.0)),
    )?))
}

impl PreviewApp {
    /// A page of `entries`, in order, or of the one [`requested_preview`] names by id when it is among them, otherwise of the requested component. A debug build panics when two of them share an id, naming where each is written.
    pub fn new(entries: Vec<PreviewEntry>) -> Self {
        if cfg!(debug_assertions)
            && let Some(duplicates) = duplicate_ids(&entries)
        {
            panic!("{duplicates}");
        }
        Self { entries }
    }

    fn page(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let mut sections: Vec<Box<dyn LayoutItem>> = Vec::new();

        let request = requested_preview();
        let requested = request
            .id
            .as_deref()
            .filter(|id| self.entries.iter().any(|entry| entry.id == *id));

        for entry in self.entries.iter().filter(|e| match requested {
            Some(id) => e.id == id,
            None => request.matches_component(e),
        }) {
            let header_text = format!("[{}]  {}", entry.component, entry.name);
            let header = Text::new(
                move || header_text.clone(),
                LayoutStyle::new().padding_all(8.0),
                || TextStyle::new(11.0, Color::rgba(0.4, 0.4, 0.55, 1.0)),
            )
            .unwrap();

            let entry = *entry;
            let canvas = remounting(
                PreviewCtx::for_entry(&entry),
                move |ctx| entry.build_root(ctx),
                failure_label,
            )
            .unwrap();

            let section = Container::new(
                LayoutStyle::new().flex_column().gap(8.0).padding_all(16.0),
                vec![
                    Box::new(header) as Box<dyn LayoutItem>,
                    Box::new(canvas) as Box<dyn LayoutItem>,
                ],
            )
            .unwrap();
            sections.push(Box::new(section));
        }

        let content = Container::new(
            LayoutStyle::new().flex_column().gap(16.0).padding_all(24.0),
            sections,
        )
        .unwrap();

        let page = ScrollPage::new(Box::new(content)).expect("page layout failed");
        Box::new(page)
    }

    /// A page light enough to read dark ink on, or dark enough to read light ink on — decided by the installed theme rather than fixed, because a component drawn for a dark surface is invisible on a light page and that reads as a broken preview rather than as a mismatched background. `ThemeTokens` has no page-background token to ask for directly, so the ink's own lightness is the proxy.
    fn page_color() -> Color {
        let ink = crate::use_theme_tokens().ink();
        let light_ink = ink.r * 0.299 + ink.g * 0.587 + ink.b * 0.114 > 0.5;
        if light_ink {
            Color::rgba(0.12, 0.12, 0.15, 1.0)
        } else {
            Color::rgba(0.96, 0.96, 0.98, 1.0)
        }
    }
}

impl App for PreviewApp {
    fn root(&self) -> Box<dyn Component> {
        self.page()
    }

    fn clear_color(&self) -> Option<Color> {
        Some(Self::page_color())
    }
}

/// Renders every preview, and every cell of its matrix, to a PNG of its own under `out_dir`, named by its id, then exits.
///
/// The third answer, and the one an out-of-tree backend wants: [`crate::try_run_test`] proves a component builds and lays out but never draws a pixel, and the preview window draws but needs a desktop window a shell has no way to open. Each preview and each cell gets its own file rather than one page of all of them, so a name identifies what it shows and a golden-image run can compare them one at a time.
///
/// Each is mounted the way a workshop canvas mounts it — on a surface of its own, in its environment, with its args at their defaults or at the cell's, settled — at the window size `config` names unless it asks for one, and drawn on the CPU rasterizer. A matrix that cannot be expanded fails once, as `<preview-id>--matrix`.
#[cfg(feature = "preview-headless")]
pub fn run_preview_png(
    entries: Vec<PreviewEntry>,
    config: AppConfig,
    out_dir: &std::path::Path,
) -> ! {
    if let Err(e) = std::fs::create_dir_all(out_dir) {
        eprintln!("cannot write previews to {}: {e}", out_dir.display());
        std::process::exit(1);
    }
    crate::runner::install_preview_text_metrics(&config);
    ui_core::open_surface_font_family(
        config
            .font_family
            .as_deref()
            .map(renderer_core::FontFamily::from),
    );
    let viewport = Size::new(
        config.window.width.max(1) as f32,
        config.window.height.max(1) as f32,
    );
    let matrices = Matrices::installed();
    println!("rendering {} preview component(s)", entries.len());

    let mut tally = Tally {
        written: 0,
        failed: fail_duplicate_ids(&entries),
    };
    for entry in &entries {
        let opening = PreviewCtx::for_entry_with(entry, Args::in_memory_for(entry));
        tally.record(
            entry.id,
            write_png(entry, entry.id, Ok(opening), viewport, out_dir),
        );
        let Some(matrix) = entry.matrix else {
            continue;
        };
        match matrix.cells(entry, &matrices) {
            Ok(cells) => {
                for cell in &cells {
                    let ctx = cell.ctx(entry).map_err(|error| error.to_string());
                    tally.record(&cell.id, write_png(entry, &cell.id, ctx, viewport, out_dir));
                }
            }
            Err(error) => {
                tally.record(&format!("{}--matrix", entry.id), Err(error.to_string()));
            }
        }
    }

    println!();
    println!(
        "preview result: {} written, {} failed",
        tally.written, tally.failed
    );
    std::process::exit(if tally.failed == 0 { 0 } else { 1 });
}

#[cfg(feature = "preview-headless")]
struct Tally {
    written: usize,
    failed: usize,
}

#[cfg(feature = "preview-headless")]
impl Tally {
    fn record(&mut self, id: &str, outcome: Result<std::path::PathBuf, String>) {
        match outcome {
            Ok(file) => {
                self.written += 1;
                println!("  ok    {id}  → {}", file.display());
            }
            Err(error) => {
                self.failed += 1;
                println!("  FAIL  {id}  {error}");
            }
        }
    }
}

/// Mounts `entry` against `ctx` and writes what its canvas shows once settled to `<out_dir>/<id>.png`: `id` is the preview's, or the cell's whose args and globals `ctx` carries.
#[cfg(feature = "preview-headless")]
fn write_png(
    entry: &PreviewEntry,
    id: &str,
    ctx: Result<PreviewCtx, String>,
    viewport: Size,
    out_dir: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let ctx = ctx?;
    let png = catch_unwind(AssertUnwindSafe(|| {
        let play = Play::mount_with(entry, ctx, viewport).map_err(|error| error.to_string())?;
        play.frame().to_png()
    }))
    .map_err(|_| "panicked while rendering".to_string())??;
    let file = out_dir.join(format!("{}.png", file_stem(id)));
    std::fs::write(&file, png).map_err(|error| error.to_string())?;
    Ok(file)
}

#[cfg(test)]
#[path = "app_test.rs"]
mod tests;
