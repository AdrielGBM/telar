//! Where `cargo telar` enters an app: the env vars that choose between running it, previewing it and testing its previews.

#[cfg(feature = "runtime")]
use crate::{LayoutError, LayoutItem};

#[cfg(all(feature = "runtime", not(target_os = "android")))]
use crate::{AppConfig, AvailableSpace, ComponentList, compute_layout};

#[cfg(feature = "runtime")]
#[derive(Clone)]
/// One `[preview]` block: which component it belongs to, its name, and the fn that builds it.
pub struct PreviewEntry {
    pub component_name: &'static str,
    pub preview_name: &'static str,
    pub build: fn() -> Result<Box<dyn LayoutItem>, LayoutError>,
    /// Set when the preview is a *surface* rather than a tree — `[preview "…" surface:360x240]`. See [`PreviewSurface`].
    pub surface: Option<PreviewSurface>,
}

/// What a preview needs to be rendered the way the runner mounts a surface, rather than as one more widget in the page's column.
///
/// A tree preview answers "does this component look right"; a surface preview answers "does this *window* look right" — and the difference is everything a surface adds on top of its content: a definite size the content lays out against, and the enter transition its root plays. Without it the two questions could not both be asked, so an app ended up keeping a headless harness of its own for the second one.
#[cfg(feature = "runtime")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewSurface {
    /// The size the surface would be given by the compositor, in logical px.
    pub width: f32,
    pub height: f32,
    /// Play the root's enter transition, so a preview shows what the user sees when the surface opens — and a transition that never settles shows up as a window that is still half-transparent when the frames run out.
    pub animate: bool,
}

#[cfg(feature = "runtime")]
impl PreviewSurface {
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            animate: false,
        }
    }

    pub fn animated(self) -> Self {
        Self {
            animate: true,
            ..self
        }
    }
}

/// The `cargo telar` dev-loop entry, for an app that wires its own runner instead of expanding [`crate::app!`] — a multi-surface host, or one on an out-of-tree backend, which reaches [`crate::run_with_platform`] or [`crate::run_multi_with_platform`] directly. `app!` generates a call to this; anything using `rsx_modules!` has to make it by hand, and until it does, `cargo telar preview`/`test` silently start the real application.
///
/// Returns `true` when it handled the invocation and the caller must return without starting its app.
///
/// `setup` runs only on the dev path, never on the way to a normal start — a caller whose setup seeds a world that exists *for* previews must not pay for it, or change its own startup, every time the app launches. It is not optional on the dev path, because [`crate::use_theme`] panics when no theme is set: a `[preview]` reading one would otherwise fail for a reason that has nothing to do with the component under test.
///
/// `entries` is a closure so a normal run pays nothing to build a list it will not read. A workspace whose `.rsx` files live in several crates concatenates one `telar_all_preview_entries()` per crate here — each `rsx_modules!` invocation emits its own, and they are per crate rather than per process.
#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub fn dev_entry<F>(entries: F, config: AppConfig, setup: impl FnOnce()) -> bool
where
    F: Fn() -> Vec<PreviewEntry>,
{
    let wanted = [
        "TELAR_PREVIEW_LIST",
        "TELAR_TEST",
        "TELAR_PREVIEW",
        "TELAR_PREVIEW_PNG",
    ]
    .iter()
    .any(|var| std::env::var(var).is_ok());
    if !wanted {
        return false;
    }
    setup();
    // Checked before `TELAR_PREVIEW`, so an app built with both hosts can ask for pictures rather than a window.
    #[cfg(feature = "preview-headless")]
    if let Ok(out_dir) = std::env::var("TELAR_PREVIEW_PNG") {
        crate::run_preview_png(entries(), config, std::path::Path::new(&out_dir));
    }
    if std::env::var("TELAR_PREVIEW_LIST").is_ok() {
        for entry in entries() {
            println!("{}\t{}", entry.component_name, entry.preview_name);
        }
        std::process::exit(0);
    }
    if std::env::var("TELAR_TEST").is_ok() {
        try_run_test(entries(), config);
    }
    if std::env::var("TELAR_PREVIEW").is_ok() {
        // Only a build with the feature can show a window; without it the caller starts its app as usual.
        #[cfg(feature = "preview")]
        {
            crate::run_app_with_name(
                config,
                crate::preview::PreviewApp { entries: entries() },
                "telar-preview",
            );
            return true;
        }
    }
    false
}

/// Renders every preview component headlessly (build → layout → flatten) and exits with a non-zero code if any panics or returns a layout error. Backs `cargo telar test`, entered via the `TELAR_TEST` env var set on the app binary.
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
        // Do not reset the runtime between components: the app's setup installed the theme once, and resetting would make previews that read theme tokens panic spuriously.
        let outcome = catch_unwind(AssertUnwindSafe(|| -> Result<usize, LayoutError> {
            crate::reset_layout_runtime();
            let item = (entry.build)()?;
            let node = item.layout_node();
            compute_layout(
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
