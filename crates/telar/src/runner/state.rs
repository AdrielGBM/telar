//! The handler's own state, in the groups it actually reads.
//!
//! Split out of `AppHandler`, which had thirty-two loose fields. What moved is what is read *together*: the clocks and flags that decide whether a frame is due, and the three values that say which application this is and where it keeps things. What stayed is what is genuinely on its own — a restart flag, an exit flag, a scratch buffer — because bagging those would name a group nothing reads as one.

use std::sync::Arc;

use services_core::AppPathsProvider;

use crate::prefs::UserPrefs;

use super::{FRAME_BUDGET, HW_KEEPALIVE_INTERVAL};
/// The clocks and flags that decide whether a frame is due, and whether the GPU should be held awake between them.
///
/// One value because they are read together and only ever mean something together: `keepalive_due` weighs three of them at once, and each was a loose field on a handler that had thirty of them.
pub(super) struct FramePacer {
    /// Whether the renderer asked to be kept warm — a GPU that sleeps costs the next frame its wake-up.
    pub(super) renderer_keepalive: bool,
    /// Whether this window has the keyboard. The keepalive rides on it: somebody who can type is somebody whose next frame should not wait for the GPU to wake up.
    pub(super) focused: bool,
    pub(super) last_input: web_time::Instant,
    pub(super) last_frame: web_time::Instant,
    /// Unlike `last_frame`, which advances only on a frame carrying new content, so the pass costs the same however often the platform calls `on_redraw`.
    pub(super) last_tick: web_time::Instant,
    /// Content or keepalive; paces the keepalive blit.
    pub(super) last_submit: web_time::Instant,
}

impl Default for FramePacer {
    fn default() -> Self {
        let now = web_time::Instant::now();
        Self {
            renderer_keepalive: false,
            // A platform that never reports focus is one whose window is the only thing on screen, so being believed focused is both the safe answer and the true one.
            focused: true,
            last_input: now,
            last_frame: now,
            // Backdated so the first `on_redraw` after resume composes immediately, and the first keepalive blit is already due. `checked_sub` because an `Instant` this early in the process may have nothing to subtract from.
            last_tick: now.checked_sub(FRAME_BUDGET).unwrap_or(now),
            last_submit: now.checked_sub(HW_KEEPALIVE_INTERVAL).unwrap_or(now),
        }
    }
}

/// Which application this is and where it keeps things: the name it is known by, the directories the platform gave it, and the preferences it remembers between runs.
///
/// One value because [`UserPrefs::save`] takes all three, and because a renderer request names two of them together — a handler holding them apart had the save site reach for `self` three times to say one thing.
pub(super) struct AppEnv {
    pub(super) app_name: String,
    pub(super) paths: Arc<dyn AppPathsProvider>,
    pub(super) prefs: UserPrefs,
}

impl AppEnv {
    /// Writes the preferences back, reporting a failure rather than losing it: a renderer the user switched to and that did not survive the restart is a setting that silently did nothing.
    pub(super) fn save_prefs(&self) {
        if let Err(e) = self.prefs.save(&self.app_name, self.paths.as_ref()) {
            tracing::warn!("failed to save prefs: {e}");
        }
    }
}
