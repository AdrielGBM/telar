#[cfg(feature = "preview-headless")]
use crate::AppConfig;
use crate::{
    App, Color, Component, Container, LayoutError, LayoutItem, LayoutStyle, PreviewEntry,
    ScrollPage, Text, TextStyle, reset_layout_runtime,
};

pub struct PreviewApp {
    pub entries: Vec<PreviewEntry>,
}

/// A tree preview is dropped into the page as it is; a surface preview is first given the two things a
/// compositor would give it — a definite size to lay out against, and the root that plays its enter transition.
///
/// The size goes on a box *around* the root rather than on the root itself: [`WindowRoot::wrapping`] fills its
/// parent by design (that is how a surface's content stretches to its window), so it needs a parent with a size.
fn mounted(
    content: Box<dyn LayoutItem>,
    surface: Option<crate::PreviewSurface>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Some(surface) = surface else {
        return Ok(content);
    };
    let root = crate::WindowRoot::wrapping(content)?;
    let root = if surface.animate {
        root.animate_in()
    } else {
        root
    };
    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .width(surface.width)
            .height(surface.height),
        vec![Box::new(root) as Box<dyn LayoutItem>],
    )?))
}

impl App for PreviewApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let mut sections: Vec<Box<dyn LayoutItem>> = Vec::new();

        // `cargo telar preview --component <name>` sets this to scope the window to one component.
        let wanted = std::env::var("TELAR_PREVIEW_COMPONENT")
            .ok()
            .filter(|s| !s.is_empty());

        for entry in self
            .entries
            .iter()
            .filter(|e| wanted.as_deref().is_none_or(|w| e.component_name == w))
        {
            let header_text = format!("[{}]  {}", entry.component_name, entry.preview_name);
            let header = Text::new(
                move || header_text.clone(),
                LayoutStyle::new().padding_all(8.0),
                || TextStyle::new(11.0, Color::rgba(0.4, 0.4, 0.55, 1.0)),
            )
            .unwrap();

            let mut children: Vec<Box<dyn LayoutItem>> = vec![Box::new(header)];
            match (entry.build)().and_then(|widget| mounted(widget, entry.surface)) {
                Ok(widget) => children.push(widget),
                Err(err) => {
                    let msg = format!("Error: {err}");
                    let label = Text::new(
                        move || msg.clone(),
                        LayoutStyle::new(),
                        || TextStyle::new(12.0, Color::rgba(0.9, 0.2, 0.2, 1.0)),
                    )
                    .unwrap();
                    children.push(Box::new(label));
                }
            }

            let section = Container::new(
                LayoutStyle::new().flex_column().gap(8.0).padding_all(16.0),
                children,
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

    /// A page light enough to read dark ink on, or dark enough to read light ink on — decided by the installed
    /// theme rather than fixed, because a component drawn for a dark surface is invisible on a light page and
    /// that reads as a broken preview rather than as a mismatched background. `ThemeTokens` has no page-background
    /// token to ask for directly, so the ink's own lightness is the proxy.
    fn clear_color(&self) -> Option<Color> {
        let ink = crate::use_theme_tokens().ink();
        let light_ink = ink.r * 0.299 + ink.g * 0.587 + ink.b * 0.114 > 0.5;
        Some(if light_ink {
            Color::rgba(0.12, 0.12, 0.15, 1.0)
        } else {
            Color::rgba(0.96, 0.96, 0.98, 1.0)
        })
    }
}

/// Renders every `[preview]` entry to its own PNG under `out_dir` on the headless backend, then exits.
///
/// The third answer, and the one an out-of-tree backend wants: [`crate::try_run_test`] proves a component builds
/// and lays out but never draws a pixel, and the preview window draws but needs a desktop window a shell has
/// no way to open. Each entry gets its own file rather than one page of all of them, so a name identifies a
/// preview and a golden-image run can compare them one at a time.
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
    let width = (config.window.width as u32).max(1);
    let height = (config.window.height as u32).max(1);
    println!("rendering {} preview component(s)", entries.len());

    let (mut written, mut failed) = (0usize, 0usize);
    for entry in entries {
        let label = format!("{}::{}", entry.component_name, entry.preview_name);
        let file = out_dir.join(format!(
            "{}.png",
            sanitize(&format!("{}-{}", entry.component_name, entry.preview_name))
        ));
        let sink: platform_headless::FrameSink = Arc::new(Mutex::new(None));
        // The headless platform paces at a real 60fps, so a handful of frames covers an enter transition settling — a preview captured on the first frame shows every animation at its start value.
        let platform = platform_headless::HeadlessPlatform::new(width, height)
            .with_frames(PREVIEW_FRAMES)
            .capture_into(sink.clone());
        let app = PreviewApp {
            entries: vec![entry],
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
        let Some(pixels) = pixels else {
            println!("  FAIL  {label}  no frame captured");
            failed += 1;
            continue;
        };
        match image::RgbaImage::from_raw(width, height, pixels)
            .ok_or_else(|| "frame size does not match the window".to_string())
            .and_then(|img| img.save(&file).map_err(|e| e.to_string()))
        {
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
