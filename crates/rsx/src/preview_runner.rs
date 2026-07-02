#[cfg(feature = "runtime")]
use crate::{LayoutError, LayoutItem, WidgetCtx};

#[cfg(all(feature = "runtime", not(target_os = "android")))]
use crate::{AppConfig, AvailableSpace, ComponentList, compute_layout};

#[cfg(feature = "runtime")]
#[derive(Clone)]
pub struct PreviewEntry {
    pub component_name: &'static str,
    pub preview_name: &'static str,
    pub build: fn(&mut WidgetCtx) -> Result<Box<dyn LayoutItem>, LayoutError>,
}

#[cfg(all(feature = "dev", feature = "preview", not(target_os = "android")))]
pub fn make_hot_preview_app(entries: Vec<PreviewEntry>) -> Box<dyn crate::App> {
    Box::new(crate::preview::PreviewApp { entries })
}

#[cfg(all(feature = "runtime", not(target_os = "android")))]
pub fn try_run_preview(entries: Vec<PreviewEntry>, config: AppConfig) -> bool {
    #[cfg(feature = "preview")]
    {
        crate::run_preview_window(entries, config);
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
