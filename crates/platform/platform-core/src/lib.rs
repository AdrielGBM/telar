//! The seam between Telar and whatever opens its windows.
//!
//! A backend implements [`Platform`] and [`Window`] against these types; everything above this crate speaks only in them, so a frontend can be swapped without touching the UI.

#![warn(rustdoc::broken_intra_doc_links)]

pub mod accessibility;
pub mod app_ctx;
pub mod consumed_keys;
pub mod error;
pub mod event;
pub mod event_sink;
#[cfg(feature = "dylib")]
pub mod guest;
pub mod history;
pub mod location;
pub mod location_format;
pub mod location_source;
pub mod loop_waker;
pub mod primary_scroll;
pub mod system_preferences;
pub mod window;
pub mod window_command;

pub use accessibility::{AccessNode, NumericValue, Role};
pub use app_ctx::{AppCtx, RedrawWaker};
pub use consumed_keys::{ConsumedKeys, consumes, key_member};
pub use error::PlatformError;
pub use event::{Event, Key, ModifiersState, NamedKey, PointerButton, PointerSource, ScrollDelta};
pub use event_sink::{post_event, set_event_sink};
pub use history::{
    HistoryFollower, HistoryFollowerId, follow_location_history, history_back, location_history,
    push_location, receive_location_history, replace_location, report_location_history,
    rewrite_location_history, unfollow_location_history,
};
pub use location::Location;
pub use location_format::{LocationFormat, location_format, set_location_format};
pub use location_source::{
    ArgumentLocation, FixedLocation, HistorySink, HistoryStep, HistoryUpdate, LocationSource,
};
pub use loop_waker::{loop_waker, set_loop_waker};
pub use primary_scroll::{
    PrimaryScroll, PrimaryScrollClaim, claim_primary_scroll, primary_scroll_offset,
    scroll_primary_to,
};
pub use system_preferences::{
    ColorScheme, SystemPreferences, locales_from_env, posix_locale_to_bcp47,
    system_locales_from_env,
};
pub use window::{
    Cursor, EventHandler, FullscreenMode, MultiSurfacePlatform, Platform, SurfaceId, Window,
    WindowConfig, WindowPosition, window_waker,
};
pub use window_command::{
    WindowCommand, WindowCommandContext, WindowCommandGuard, push_window_command,
    take_window_commands,
};
