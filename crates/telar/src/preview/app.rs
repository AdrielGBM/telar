//! The fallback preview host: every preview in one scrolling column, for a build with no workshop.

use std::cell::RefCell;
use std::rc::Rc;

#[cfg(feature = "preview-headless")]
use crate::AppConfig;
use crate::{
    App, BuildFailure, Color, Component, Container, LayoutError, LayoutItem, LayoutStyle,
    ScrollPage, Text, TextStyle, reset_layout_runtime,
};

#[cfg(feature = "preview-headless")]
use super::host::fail_duplicate_ids;
use super::host::{duplicate_ids, remounting, requested_preview};
use super::{PreviewCtx, PreviewEntry};

/// An app that renders previews instead of its own root: the page `cargo telar preview` falls back to when the package declares no workshop.
///
/// Each preview is built inside its own [`crate::ErrorBoundary`], so one that fails or panics shows its error in place and the rest of the page keeps running, and is built again when an arg it read changes.
pub struct PreviewApp {
    entries: Vec<PreviewEntry>,
}

/// The failures a page's boundaries caught, for a caller that has to report them rather than only show them.
type Failures = Rc<RefCell<Vec<String>>>;

fn failure_label(
    failure: BuildFailure,
    record: Option<&Failures>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let message = failure.to_string();
    if let Some(record) = record {
        record.borrow_mut().push(message.clone());
    }
    let message = format!("Error: {message}");
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

    fn page(&self, failures: Option<&Failures>) -> Box<dyn Component> {
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
            let record = failures.cloned();
            let canvas = remounting(
                PreviewCtx::for_entry(&entry),
                move |ctx| entry.build_root(ctx),
                move |failure| failure_label(failure, record.as_ref()),
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
        self.page(None)
    }

    fn clear_color(&self) -> Option<Color> {
        Some(Self::page_color())
    }
}

/// [`PreviewApp`] for one PNG, keeping what its boundary caught so a preview that failed is reported as a failure rather than written as a picture of its error.
#[cfg(feature = "preview-headless")]
struct PngApp {
    page: PreviewApp,
    failures: Failures,
}

#[cfg(feature = "preview-headless")]
impl App for PngApp {
    fn root(&self) -> Box<dyn Component> {
        self.page.page(Some(&self.failures))
    }

    fn clear_color(&self) -> Option<Color> {
        Some(PreviewApp::page_color())
    }
}

/// Renders every preview to its own PNG under `out_dir`, named by its id, on the headless backend, then exits.
///
/// The third answer, and the one an out-of-tree backend wants: [`crate::try_run_test`] proves a component builds and lays out but never draws a pixel, and the preview window draws but needs a desktop window a shell has no way to open. Each entry gets its own file rather than one page of all of them, so a name identifies a preview and a golden-image run can compare them one at a time.
#[cfg(feature = "preview-headless")]
pub fn run_preview_png(
    entries: Vec<PreviewEntry>,
    config: AppConfig,
    out_dir: &std::path::Path,
) -> ! {
    use std::sync::{Arc, Mutex};

    if let Err(e) = std::fs::create_dir_all(out_dir) {
        eprintln!("cannot write previews to {}: {e}", out_dir.display());
        std::process::exit(1);
    }
    let width = config.window.width.max(1);
    let height = config.window.height.max(1);
    println!("rendering {} preview component(s)", entries.len());

    let (mut written, mut failed) = (0usize, fail_duplicate_ids(&entries));
    for entry in entries {
        let label = entry.id;
        let file = out_dir.join(format!("{}.png", sanitize(entry.id)));
        let sink: platform_headless::FrameSink = Arc::new(Mutex::new(None));
        // The headless platform paces at a real 60fps, so a handful of frames lets an enter transition settle — a preview captured on the first frame shows every animation at its start value.
        let platform = platform_headless::HeadlessPlatform::new(width, height)
            .with_frames(PREVIEW_FRAMES)
            .capture_into(sink.clone());
        let failures = Failures::default();
        let app = PngApp {
            page: PreviewApp::new(vec![entry]),
            failures: Rc::clone(&failures),
        };
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::run_with_platform::<_, _, ()>(
                platform,
                config.clone(),
                Arc::new(crate::NoPaths) as Arc<dyn crate::AppPathsProvider>,
                app,
                "telar-preview",
            )
        }));
        let pixels = match outcome {
            Ok(Ok(())) => sink.lock().ok().and_then(|mut held| held.take()),
            Ok(Err(e)) => {
                println!("  FAIL  {label}  {e}");
                failed += 1;
                continue;
            }
            Err(_) => {
                println!("  FAIL  {label}  panicked while rendering");
                failed += 1;
                continue;
            }
        };
        if let Some(failure) = failures.borrow().first() {
            println!("  FAIL  {label}  {failure}");
            failed += 1;
            continue;
        }
        let Some(pixels) = pixels else {
            println!("  FAIL  {label}  no frame captured");
            failed += 1;
            continue;
        };
        match renderer_software::save_premultiplied_rgba8_png(pixels, width, height, &file) {
            Ok(()) => {
                written += 1;
                println!("  ok    {label}  → {}", file.display());
            }
            Err(e) => {
                failed += 1;
                println!("  FAIL  {label}  {e}");
            }
        }
    }

    println!();
    println!("preview result: {written} written, {failed} failed");
    std::process::exit(if failed == 0 { 0 } else { 1 });
}

/// Enough frames at 60fps for a 200ms enter transition to settle.
#[cfg(feature = "preview-headless")]
const PREVIEW_FRAMES: u32 = 13;

#[cfg(feature = "preview-headless")]
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "app_test.rs"]
mod tests;
