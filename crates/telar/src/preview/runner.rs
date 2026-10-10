//! Where `cargo telar` enters an app: the env vars that choose between running it, previewing it and testing its previews.

use telar_project::protocol::TEST_VAR;

use crate::AppConfig;

use super::PreviewEntry;
use super::host::duplicate_ids;
use super::test_run::{TestOptions, run_previews};

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
        TEST_VAR,
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
    if std::env::var(TEST_VAR).is_ok() {
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

/// Tests every preview and every cell of its matrix — built and laid out, played, checked for accessibility, compared against its snapshots — as the `TELAR_TEST_*` variables `cargo telar test` sets ask, prints a line for each, and exits non-zero if any failed or shares its id with another. Backs `cargo telar test`, entered via the `TELAR_TEST` env var set on the app binary.
pub fn try_run_test(entries: Vec<PreviewEntry>, config: AppConfig) -> ! {
    let options = match TestOptions::from_env() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let run = run_previews(entries, &config, &options);
    std::process::exit(if run.passed() { 0 } else { 1 });
}
