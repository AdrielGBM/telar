//! Which frontend an app runs on.

use crate::app::App;
use crate::app_config::AppConfig;

/// Runs `app` on the frontend this build has.
///
/// Which one that is is a *build* decision: a binary compiled with `desktop` opens a window, one compiled
/// with only `tui` runs in the terminal it was launched from. Both being compiled in is the case a dev
/// session wants — switching frontend without a rebuild — and there `TELAR_TARGET=tui` picks the terminal,
/// with the window remaining the default.
///
/// This is what [`telar::app!`](telar_macros::app)'s generated `run()` calls, so a `.rsx` app reaches every
/// frontend from one entry point and one source tree.
pub fn run_app_with_name<A: App>(config: AppConfig, app: A, app_name: &str) {
    #[cfg(feature = "tui")]
    if tui_selected() {
        super::run_tui_app_with_name(config, super::TuiOptions::default(), app, app_name);
        return;
    }
    run_windowed(config, app, app_name)
}

/// Whether this run should go to the terminal: because it is the only frontend compiled in, or because it
/// was asked for.
#[cfg(feature = "tui")]
fn tui_selected() -> bool {
    if cfg!(not(all(feature = "desktop", not(target_os = "android")))) {
        return true;
    }
    std::env::var("TELAR_TARGET").is_ok_and(|target| target.eq_ignore_ascii_case("tui"))
}

#[cfg(all(feature = "desktop", not(target_os = "android")))]
fn run_windowed<A: App>(config: AppConfig, app: A, app_name: &str) {
    super::desktop::run_desktop_app_with_name(config, app, app_name)
}

#[cfg(not(all(feature = "desktop", not(target_os = "android"))))]
fn run_windowed<A: App>(_config: AppConfig, _app: A, _app_name: &str) {
    // Reachable only from a build with no windowing frontend *and* no terminal one, i.e. a binary that
    // enabled neither: `run_app_with_name` on a `tui`-only build never gets here.
    panic!(
        "this build of telar has no frontend to run on — enable the `desktop` feature for a window, or \
         `tui` for the terminal"
    );
}
