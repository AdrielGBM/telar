//! Where `cargo telar` enters an app: the env vars that choose between running it, previewing it and testing its previews.

use crate::{
    AppConfig, AvailableSpace, ComponentList, LayoutError, compute_layout, dispose_owner,
    owner_scope,
};

use super::host::{duplicate_ids, fail_duplicate_ids};
use super::{PreviewCtx, PreviewEntry};

/// The `cargo telar` dev-loop entry, for an app that wires its own runner instead of expanding [`crate::app!`] — a multi-surface host, or one on an out-of-tree backend, which reaches [`crate::run_with_platform`] or [`crate::run_multi_with_platform`] directly. `app!` generates a call to this; anything using `rsx_modules!` has to make it by hand, and until it does, `cargo telar preview`/`test` silently start the real application.
///
/// Returns `true` when it handled the invocation and the caller must return without starting its app.
///
/// `setup` runs only on the dev path, never on the way to a normal start — a caller whose setup seeds a world that exists *for* previews must not pay for it, or change its own startup, every time the app launches. It is not optional on the dev path, because [`crate::use_theme`] panics when no theme is set: a `[preview]` reading one would otherwise fail for a reason that has nothing to do with the component under test.
///
/// `entries` is a closure so a normal run pays nothing to build a list it will not read. `app!` passes the crate's `telar_all_previews` followed by each `[telar.previews] include` crate's; a runner wired by hand passes whatever list it builds, usually its own `telar_all_previews`.
///
/// `shell` builds the app that shows the previews in a window under `cargo telar preview`: [`PreviewApp::new`](super::host::PreviewApp::new), or a workshop's. It reads the preview or component to open first, if any, from [`requested_preview`](super::host::requested_preview).
pub fn dev_entry<F, S, A>(entries: F, config: AppConfig, setup: impl FnOnce(), shell: S) -> bool
where
    F: Fn() -> Vec<PreviewEntry>,
    S: FnOnce(Vec<PreviewEntry>) -> A,
    A: crate::App + 'static,
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
        let entries = entries();
        for entry in &entries {
            println!("{}", entry.id);
        }
        if let Some(duplicates) = duplicate_ids(&entries) {
            eprintln!("{duplicates}");
            std::process::exit(1);
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
            let entries = entries();
            if let Some(duplicates) = duplicate_ids(&entries) {
                eprintln!("{duplicates}");
                std::process::exit(1);
            }
            crate::run_app_with_name(config, shell(entries), "telar-preview");
            return true;
        }
    }
    let _ = shell;
    false
}

/// Renders every preview component headlessly (build → layout → flatten) and exits with a non-zero code if any panics, returns a layout error or shares its id with another. Backs `cargo telar test`, entered via the `TELAR_TEST` env var set on the app binary.
pub fn try_run_test(entries: Vec<PreviewEntry>, config: AppConfig) -> ! {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let width = config.window.width as f32;
    let height = config.window.height as f32;
    crate::runner::install_preview_text_metrics(&config);
    ui_core::open_surface_font_family(
        config
            .font_family
            .as_deref()
            .map(renderer_core::FontFamily::from),
    );
    println!("running {} preview component(s)", entries.len());

    let mut passed = 0usize;
    let mut failed = fail_duplicate_ids(&entries);
    for entry in &entries {
        let label = entry.id;
        // The runtime is not reset between previews, since the app's setup installed the theme once; each gets an owner of its own instead, disposed below, because `reset_layout_runtime` recycles node ids and an effect left behind would reconcile onto whatever now holds its old container id, attaching a node under its own descendant.
        let scope = owner_scope();
        let owner = scope.id();
        let outcome = catch_unwind(AssertUnwindSafe(|| -> Result<usize, LayoutError> {
            crate::reset_layout_runtime();
            ui_core::set_surface_size(geometry_core::Size::new(width, height));
            let item = entry.build_root(&PreviewCtx::for_entry(entry))?;
            let node = item.layout_node();
            compute_layout(
                node,
                AvailableSpace::Definite(width),
                AvailableSpace::Definite(height),
            )?;
            let tree = ComponentList::new(item);
            Ok(tree.commands().len())
        }));
        // Before the next `reset_layout_runtime`, so a withdrawing child still detaches from a tree that exists.
        drop(scope);
        dispose_owner(owner);
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
