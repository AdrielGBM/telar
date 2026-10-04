//! Writing a page of the app ahead of time, with no browser: `cargo telar build --prerender` runs the app's own binary once per page with [`PRERENDER_ENV`] set, and this is what the binary does instead of opening a window.
//!
//! The tree is built the way the runner builds it on a surface — the same steps, in the same order, from the same inputs — so a client that starts from the inputs the page carries builds the same layout nodes, and so the same element ids. See `docs/prerender.md`.

use std::sync::Arc;

use platform_core::{ColorScheme, Event, Location, SystemPreferences};
use services_core::NoPaths;
use telar_project::{
    PRERENDER_ENV, PageLocation, PageRequest, Preferences, PrerenderRequest, PrerenderState,
    PrerenderedPage, STATE_VERSION,
};
use web_time::Instant;

use crate::app::App;
use crate::app_config::AppConfig;
use crate::app_runtime::{AppRuntime, LocalApp};
use crate::runner::font_config::{FontSetup, SystemFonts, build_font_config};

/// The frames a page is given to settle in, as the runner's clock counts them: ten seconds of motion.
const MAX_FRAMES: u32 = 600;

/// What the address of a page the app has no page for is built from: a segment no route reads.
const NOT_FOUND_SEGMENT: &str = "\u{0}telar-not-found";

/// The request this process was started with, when it was started to write a page.
pub(super) fn requested() -> Option<PrerenderRequest> {
    let raw = std::env::var(PRERENDER_ENV).ok()?;
    match serde_json::from_str(&raw) {
        Ok(request) => Some(request),
        Err(e) => {
            eprintln!("[telar] {PRERENDER_ENV} is not a prerender request: {e}");
            std::process::exit(2);
        }
    }
}

/// Writes the page `request` names where it says, and ends the process.
pub(super) fn run<A: App>(
    config: AppConfig,
    app: A,
    app_name: &str,
    request: PrerenderRequest,
) -> ! {
    let page = prerender_page(config, app, app_name, &request);
    let written = serde_json::to_vec(&page)
        .map_err(|e| e.to_string())
        .and_then(|json| std::fs::write(&request.out, json).map_err(|e| e.to_string()));
    match written {
        Ok(()) => std::process::exit(0),
        Err(e) => {
            eprintln!("[telar] could not write {}: {e}", request.out.display());
            std::process::exit(1);
        }
    }
}

/// Builds `app` at the page `request` names, lets it settle, and writes the frame it settled on as a document.
///
/// Call it once per process: it installs what the runner would, and a second app built in the same thread inherits the first one's signals.
pub fn prerender_page<A: App>(
    config: AppConfig,
    app: A,
    app_name: &str,
    request: &PrerenderRequest,
) -> PrerenderedPage {
    let previous_capture = ui_tree::set_element_capture(true);
    services_core::app_paths::install(app_name, Arc::new(NoPaths));
    crate::user_preferences::install(app_name);
    let AppConfig {
        window,
        fonts: faces,
        font_family,
    } = config;
    renderer_text::fonts::install(build_font_config(
        FontSetup {
            faces,
            family: font_family,
        },
        &SystemFonts::from_provider(&NoPaths),
    ));
    let window = super::resolved_window(window, &app);
    let mut app = LocalApp(app);
    let surface = request.surface;

    platform_core::set_location_format(platform_core::LocationFormat::new(&request.base));
    platform_core::set_asset_base(&request.base);
    app.set_system_preferences(&system_preferences(&request.preferences));
    let size = geometry_core::Size::new(surface.width as f32, surface.height as f32);
    app.set_surface_size(size);
    app.set_location_history(&[location_of(&request.page)]);
    app.open_title(&window.title, &window.title);
    let mut tree = app.mount();
    app.set_surface_size(size);
    tree.on_event(&Event::WindowResized {
        width: surface.width,
        height: surface.height,
    });

    let settled = settle(&mut app, tree.as_ref());
    let commands = tree.frame().to_vec();
    let document = renderer_dom::prerender(&commands, app.clear_color());
    let page = PrerenderedPage {
        lang: i18n_core::current_locale(),
        title: ui_core::surface_title(),
        host_attributes: document.host_attributes,
        head: renderer_dom::reset_stylesheet() + &document.head,
        markup: document.markup,
        state: PrerenderState {
            version: STATE_VERSION,
            location: match request.page {
                PageRequest::At { .. } => current_address(),
                PageRequest::NotFound => None,
            },
            locale: i18n_core::current_locale(),
            preferences: request.preferences.clone(),
            surface,
            signals: keyed_signals(),
        },
        pages: platform_core::location_pages()
            .iter()
            .map(|location| PageLocation {
                segments: location.segments().to_vec(),
                locale: None,
            })
            .collect(),
        locales: platform_core::location_locales(),
        base_locale: crate::location_locale::location_base_locale(),
        settled,
    };
    drop(tree);
    ui_tree::set_element_capture(previous_capture);
    page
}

/// Runs frames on the runner's clock until two in a row compose the same commands and nothing is animating, and says whether that happened before [`MAX_FRAMES`].
fn settle(app: &mut LocalApp<impl App>, tree: &dyn crate::tree::UiTree) -> bool {
    let start = Instant::now();
    let mut last = None;
    for frame in 0..MAX_FRAMES {
        reactive_core::begin_batch();
        app.drain_tasks();
        app.motion_tick(start + super::FRAME_BUDGET * frame);
        reactive_core::end_batch();
        app.relayout();
        let mut redraw_requested = false;
        app.on_frame(&mut platform_core::AppCtx::new(
            &mut redraw_requested,
            None,
            None,
            None,
        ));
        ui_core::end_keyboard_frame();
        let generation = tree.generation();
        if last == Some(generation) && !app.motion_has_active() {
            return true;
        }
        last = Some(generation);
    }
    tracing::warn!(
        "the page was still changing after {}s of frames; writing the frame it reached",
        (super::FRAME_BUDGET * MAX_FRAMES).as_secs_f32()
    );
    false
}

/// The preferences a reader nobody has asked yet is assumed to have, as the platform would report them.
fn system_preferences(preferences: &Preferences) -> SystemPreferences {
    SystemPreferences {
        color_scheme: preferences
            .color_scheme
            .as_deref()
            .and_then(|scheme| match scheme {
                "light" => Some(ColorScheme::Light),
                "dark" => Some(ColorScheme::Dark),
                _ => None,
            }),
        reduced_motion: preferences.reduced_motion,
        high_contrast: preferences.high_contrast,
        locales: preferences.locales.clone(),
    }
}

fn location_of(page: &PageRequest) -> Location {
    match page {
        PageRequest::At { location } => {
            let at = Location::from_segments(location.segments.iter().cloned());
            match &location.locale {
                Some(locale) => at.with_locale(locale.clone()),
                None => at,
            }
        }
        PageRequest::NotFound => Location::root().segment(NOT_FOUND_SEGMENT),
    }
}

/// The address the app settled on, written as it writes its own with no base path: a page asked for in no locale is written in the one the app chose.
fn current_address() -> Option<String> {
    let current = platform_core::location_history().last()?.clone();
    let format =
        platform_core::LocationFormat::root().with_locales(platform_core::location_locales());
    Some(format.format(&current.without_fragment()))
}

/// Every keyed signal the tree read, and the theme's choices, as the values they had when the page was written.
fn keyed_signals() -> std::collections::BTreeMap<String, String> {
    serde_json::from_str(&crate::hot_state::hot_snapshot_json()).unwrap_or_default()
}

#[cfg(test)]
#[path = "prerender_test.rs"]
mod tests;
