//! Starting from a page that was prerendered: the first tree is built from the inputs the page was written from, so it opens the same layout nodes and the document renderer can take the page over element by element. What the browser really reports follows as an ordinary update.
//!
//! The state is the `telar-state` script `cargo telar build --prerender` writes (`telar_project::PrerenderState`). The browser reads it (`platform_web::served_state`) without the packaging crate or a JSON parser of the app's own; `hydration_test.rs` holds the packager to the format it reads.

use platform_core::SystemPreferences;

/// The `id` of the script element a prerendered page carries its inputs in.
#[cfg_attr(
    not(all(feature = "web-dom", target_arch = "wasm32")),
    allow(dead_code)
)]
pub(crate) const STATE_ELEMENT_ID: &str = "telar-state";

/// The format of the state this client reads. A page that carries another is built from nothing.
#[cfg_attr(
    not(all(feature = "web-dom", target_arch = "wasm32")),
    allow(dead_code)
)]
pub(crate) const STATE_VERSION: u32 = 1;

/// How many rounds of instant motion the first tree is given to reach the frame the page was written from, for an animation that starts another as it ends.
const SETTLE_ROUNDS: usize = 8;

/// What a prerendered page was built from, as the first tree is built from it.
#[cfg_attr(
    not(all(feature = "web-dom", target_arch = "wasm32")),
    allow(dead_code)
)]
pub(crate) struct Hydration {
    /// The system preferences the page assumed.
    pub(crate) preferences: SystemPreferences,
    /// The surface size the page was laid out at, in CSS pixels.
    pub(crate) surface: (u32, u32),
    locale: Option<String>,
    signals: Vec<(String, String)>,
    /// What the platform reports before the tree is built, held back until it has been built from what the page assumed.
    pub(crate) reported: Option<SystemPreferences>,
}

#[cfg_attr(
    not(all(feature = "web-dom", target_arch = "wasm32")),
    allow(dead_code)
)]
impl Hydration {
    /// The inputs of the prerendered page the document was served with, when it carries a state this client reads.
    #[cfg(all(feature = "web-dom", target_arch = "wasm32"))]
    pub(crate) fn served() -> Option<Self> {
        let state = platform_web::served_state(STATE_ELEMENT_ID)?;
        Self::new(
            state.version,
            state.preferences,
            state.surface,
            state.locale,
            state.signals,
        )
    }

    /// The inputs a page's state names, or `None` for one written in another format.
    pub(crate) fn new(
        version: u32,
        preferences: SystemPreferences,
        surface: (u32, u32),
        locale: Option<String>,
        signals: Vec<(String, String)>,
    ) -> Option<Self> {
        if version != STATE_VERSION {
            tracing::warn!(
                "the page's {STATE_ELEMENT_ID} is version {version}, and this client reads {STATE_VERSION}, so it is built anew"
            );
            return None;
        }
        Some(Self {
            preferences,
            surface,
            locale,
            signals,
            reported: None,
        })
    }

    /// Puts the page's locale and keyed signals in place, ahead of the tree that reads them.
    pub(crate) fn restore(&self) {
        if let Some(locale) = &self.locale {
            i18n_core::set_locale(locale.clone());
        }
        #[cfg(any(
            feature = "dev",
            feature = "prerender",
            all(feature = "web-dom", target_arch = "wasm32")
        ))]
        crate::hot_state::restore(self.signals.iter().cloned());
    }
}

/// Runs every animation the first tree started to its end, so the first frame shows what the page was written showing: it was written once the app had settled, so an entrance has already played for the reader.
pub(crate) fn settle_motion(app: &dyn crate::app_runtime::AppRuntime) {
    let scale = motion_core::scale();
    motion_core::set_scale(0.0);
    let now = web_time::Instant::now();
    for _ in 0..SETTLE_ROUNDS {
        app.drain_tasks();
        app.motion_tick(now);
        reactive_core::end_batch();
        app.relayout();
        reactive_core::begin_batch();
        if !app.motion_has_active() {
            break;
        }
    }
    motion_core::set_scale(scale);
}

#[cfg(all(test, feature = "prerender"))]
#[path = "hydration_test.rs"]
mod tests;
