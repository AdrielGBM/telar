//! The seam between Telar and whatever opens its windows.
//!
//! A backend implements [`Platform`] and [`Window`] against these types; everything above this crate speaks only in them, so a frontend can be swapped without touching the UI.

#![warn(rustdoc::broken_intra_doc_links)]

pub mod accessibility;
pub mod app_ctx;
pub mod asset_base;
pub mod consumed_keys;
pub mod destination;
pub mod error;
pub mod event;
pub mod event_sink;
#[cfg(feature = "dylib")]
pub mod guest;
pub mod history;
pub mod location_format;
pub mod location_source;
pub mod loop_waker;
pub mod primary_scroll;
pub mod route;
pub mod system_preferences;
pub mod window;
pub mod window_command;

pub use accessibility::{AccessNode, NumericValue, Role, ToggleKind};
pub use app_ctx::{AppCtx, RedrawWaker};
pub use asset_base::{asset_url, set_asset_base};
pub use consumed_keys::{ConsumedKeys, consumes, key_member};
pub use destination::{Destination, IntoDestination, Uri, address_of, anchor, external, in_locale};
pub use error::PlatformError;
pub use event::{
    Event, Key, ModifiersState, NamedKey, PointerButton, PointerSource, ScrollDelta, TAP_SLOP,
};
pub use event_sink::{post_event, set_event_sink};
pub use history::{
    HistoryFollower, HistoryFollowerId, LocaleFollower, bind_location_locale,
    follow_location_history, history_back, is_following_location_history, location_history,
    location_locale, location_locales, location_pages, pages_of, push_anchor, push_location,
    receive_location_history, rename_anchor, replace_location, report_location_history,
    rewrite_location_history, set_anchor_revealer, set_location_locale, unfollow_location_history,
};
pub use location_format::{LocationFormat, location_format, set_location_format};
pub use location_source::{
    ArgumentLocation, FixedLocation, HistorySink, HistoryStep, HistoryUpdate, LocationSource,
};
pub use loop_waker::{loop_waker, set_loop_waker};
pub use primary_scroll::{
    PrimaryScroll, PrimaryScrollClaim, claim_primary_scroll, primary_scroll_offset,
    scroll_primary_to,
};
pub use route::Route;
pub use semantics_core::Location;
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
