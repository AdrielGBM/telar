//! Which frontend an app runs on.
//!
//! The three predicates below are spelled out at every use rather than named once, because a `cfg` attribute takes no macro: `all(feature = "desktop", not(target_os = "android"), not(target_arch = "wasm32"))` is a window, `all(feature = "web-dom", target_arch = "wasm32")` is a page, and `feature = "tui"` is a terminal.

use crate::app::App;
use crate::app_config::AppConfig;

/// Runs `app` on the frontend this build has.
///
/// Which one that is is a *build* decision: a binary compiled with `desktop` opens a window, a `wasm32` one compiled with `web` mounts on a page, one compiled with only `tui` runs in the terminal it was launched from. Both a window and a terminal being compiled in is the case a dev session wants — switching frontend without a rebuild — and there `TELAR_TARGET=tui` picks the terminal, with the window remaining the default.
///
/// This is what [`telar::app!`](telar_macros::app)'s generated `run()` calls, so a `.rsx` app reaches every frontend from one entry point and one source tree.
pub fn run_app_with_name<A: App>(config: AppConfig, app: A, app_name: &str) {
    #[cfg(feature = "tui")]
    if tui_selected() {
        super::run_tui_app_with_name(config, super::TuiOptions::default(), app, app_name);
        return;
    }
    run_default_frontend(config, app, app_name)
}

/// Whether this run should go to the terminal: because it is the only frontend compiled in, or because it was asked for.
#[cfg(feature = "tui")]
fn tui_selected() -> bool {
    let has_window = cfg!(all(
        feature = "desktop",
        not(target_os = "android"),
        not(target_arch = "wasm32")
    ));
    let has_page = cfg!(all(feature = "web-dom", target_arch = "wasm32"));
    if !has_window && !has_page {
        return true;
    }
    std::env::var("TELAR_TARGET").is_ok_and(|target| target.eq_ignore_ascii_case("tui"))
}

#[cfg(all(
    feature = "desktop",
    not(target_os = "android"),
    not(target_arch = "wasm32")
))]
fn run_default_frontend<A: App>(config: AppConfig, app: A, app_name: &str) {
    super::desktop::run_desktop_app_with_name(config, app, app_name)
}

#[cfg(all(
    feature = "web-dom",
    target_arch = "wasm32",
    not(all(
        feature = "desktop",
        not(target_os = "android"),
        not(target_arch = "wasm32")
    ))
))]
fn run_default_frontend<A: App>(config: AppConfig, app: A, app_name: &str) {
    super::run_web_app_with_name(config, super::WebOptions::default(), app, app_name)
}

#[cfg(not(any(
    all(
        feature = "desktop",
        not(target_os = "android"),
        not(target_arch = "wasm32")
    ),
    all(feature = "web-dom", target_arch = "wasm32")
)))]
fn run_default_frontend<A: App>(_config: AppConfig, _app: A, _app_name: &str) {
    // Reachable only from a build with no frontend at all: a `tui`-only build never gets here, because `tui_selected` answers `true` when nothing else is compiled in.
    panic!(
        "this build of telar has no frontend to run on — enable `desktop` for a window, `web` for a page, \
         or `tui` for the terminal"
    );
}
