//! `cargo telar preview --png`: every preview, and every cell of its matrix, drawn to a PNG of its own.

use crate::{AppConfig, Size};

use super::host::{Args, fail_duplicate_ids, file_stem};
use super::{Matrices, Play, PreviewCtx, PreviewEntry};

/// Renders every preview, and every cell of its matrix, to a PNG of its own under `out_dir`, named by its id, then exits.
///
/// The third answer, and the one an out-of-tree backend wants: [`crate::try_run_test`] judges each preview against its goldens rather than handing over a picture to look at, and the workshop draws but needs a window a shell has no way to open. Each preview and each cell gets its own file rather than one page of all of them, so a name identifies what it shows and a golden-image run can compare them one at a time.
///
/// Each is mounted the way a workshop canvas mounts it — on a surface of its own, in its environment, with its args at their defaults or at the cell's, settled — at the window size `config` names unless it asks for one, and drawn on the CPU rasterizer. A matrix that cannot be expanded fails once, as `<preview-id>--matrix`.
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

struct Tally {
    written: usize,
    failed: usize,
}

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
