//! Where `cargo telar` enters an app: the env vars that choose between running it, previewing it and testing its previews, and, in a browser, the build itself.

use telar_project::protocol::{ListedPreview, PREVIEW_LIST_FILE_VAR, PREVIEW_LIST_VAR, TEST_VAR};

use crate::AppConfig;

use super::PreviewEntry;
use super::host::duplicate_ids;
use super::test_run::{TestOptions, run_previews};

/// The `cargo telar` dev-loop entry, for an app that wires its own runner instead of expanding [`crate::app!`] — a multi-surface host, or one on an out-of-tree backend, which reaches [`crate::run_with_platform`] or [`crate::run_multi_with_platform`] directly. `app!` generates a call to this; anything using `rsx_modules!` has to make it by hand, and until it does, `cargo telar preview`/`test` silently start the real application.
///
/// It answers the requests a build with previews takes without a window: listing them, testing them, and drawing them to PNGs. A build with the preview host goes on to [`show_previews`](super::host::show_previews) when this declines, so an app built with both can still ask for pictures rather than a window.
///
/// Returns `true` when it handled the invocation and the caller must return without starting its app.
///
/// `setup` runs only on the dev path, never on the way to a normal start — a caller whose setup seeds a world that exists *for* previews must not pay for it, or change its own startup, every time the app launches. It is not optional on the dev path, because [`crate::use_theme`] panics when no theme is set: a `[preview]` reading one would otherwise fail for a reason that has nothing to do with the component under test.
///
/// `entries` is a closure so a normal run pays nothing to build a list it will not read. `app!` passes the crate's `telar_all_previews` followed by each `[telar.previews] include` crate's; a runner wired by hand passes whatever list it builds, usually its own `telar_all_previews`.
pub fn dev_entry<F>(entries: F, config: AppConfig, setup: impl FnOnce()) -> bool
where
    F: Fn() -> Vec<PreviewEntry>,
{
    let Some(request) = Request::asked() else {
        return false;
    };
    setup();
    super::host::speak_the_app_language();
    match request {
        #[cfg(feature = "preview-headless")]
        Request::Png(out_dir) => {
            crate::run_preview_png(entries(), config, std::path::Path::new(&out_dir))
        }
        Request::List(file) => list(&entries(), file.as_deref()),
        Request::Test => try_run_test(entries(), config),
    }
}

/// Opens the previews in `shell`, the app `cargo telar preview` shows them in, when it asked for them, and returns `true` when it did. `entries` and `setup` are [`dev_entry`]'s.
///
/// `app!` passes the workshop's `WorkshopApp`, from the package's optional `telar-workshop` dependency, and refuses to compile a build with the preview host that lacks it. `shell` reads the preview or component to open first, if any, from [`requested_preview`](super::host::requested_preview).
///
/// A page has no environment to be asked through, so a browser build answers from the build alone: one with the preview host, which only `cargo telar preview --target web` makes, shows the previews as it loads.
#[cfg(feature = "preview")]
pub fn show_previews<F, S, A>(entries: F, config: AppConfig, setup: impl FnOnce(), shell: S) -> bool
where
    F: Fn() -> Vec<PreviewEntry>,
    S: FnOnce(Vec<PreviewEntry>) -> A,
    A: crate::App + 'static,
{
    if !cfg!(target_arch = "wasm32") && std::env::var("TELAR_PREVIEW").is_err() {
        return false;
    }
    setup();
    super::host::speak_the_app_language();
    let entries = entries();
    if let Some(duplicates) = duplicate_ids(&entries) {
        refuse(&duplicates);
    }
    crate::run_app_with_name(config, shell(entries), "telar-workshop");
    true
}

/// What `cargo telar` asked the app to do in place of starting, short of showing the previews.
enum Request {
    #[cfg(feature = "preview-headless")]
    Png(String),
    List(Option<String>),
    Test,
}

impl Request {
    /// Checked in this order, so a build with the PNG renderer can be asked for pictures whatever else is set.
    fn asked() -> Option<Self> {
        if cfg!(target_arch = "wasm32") {
            return None;
        }
        #[cfg(feature = "preview-headless")]
        if let Ok(out_dir) = std::env::var("TELAR_PREVIEW_PNG") {
            return Some(Self::Png(out_dir));
        }
        let set = |var: &str| std::env::var(var).is_ok();
        if set(PREVIEW_LIST_VAR) {
            return Some(Self::List(std::env::var(PREVIEW_LIST_FILE_VAR).ok()));
        }
        set(TEST_VAR).then_some(Self::Test)
    }
}

/// Prints one preview id per line, or writes every preview's id and title to `file` as JSON, and exits, failing when two previews share an id.
fn list(entries: &[PreviewEntry], file: Option<&str>) -> ! {
    match file {
        Some(file) => write_listing(entries, file),
        None => {
            for entry in entries {
                println!("{}", entry.id);
            }
        }
    }
    if let Some(duplicates) = duplicate_ids(entries) {
        refuse(&duplicates);
    }
    std::process::exit(0);
}

fn write_listing(entries: &[PreviewEntry], file: &str) {
    let listed: Vec<ListedPreview> = entries
        .iter()
        .map(|entry| ListedPreview {
            id: entry.id.to_string(),
            package: entry.package.to_string(),
            title: entry.title.to_string(),
        })
        .collect();
    let written = serde_json::to_vec(&listed)
        .map_err(|e| e.to_string())
        .and_then(|json| std::fs::write(file, json).map_err(|e| e.to_string()));
    if let Err(e) = written {
        refuse(&format!(
            "could not write the preview listing to {file}: {e}"
        ));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn refuse(duplicates: &str) -> ! {
    eprintln!("{duplicates}");
    std::process::exit(1);
}

/// A page has no exit code to fail with and no terminal to print to, so the module stops on a panic, which the browser's console shows.
#[cfg(target_arch = "wasm32")]
fn refuse(duplicates: &str) -> ! {
    #[cfg(feature = "web-dom")]
    platform_web::install_console_logging();
    panic!("{duplicates}");
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
