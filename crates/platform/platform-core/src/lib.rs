//! The seam between Telar and whatever opens its windows.
//!
//! A backend implements [`Platform`] and [`Window`] against these types; everything above this crate speaks only in them, so a frontend can be swapped without touching the UI.

#![warn(rustdoc::broken_intra_doc_links)]

pub mod access_audit;
pub mod accessibility;
pub mod app_ctx;
pub mod asset_base;
pub mod chord;
pub mod consumed_keys;
pub mod destination;
pub mod dom_code;
pub mod error;
pub mod event;
pub mod event_sink;
pub mod guest;
pub mod history;
pub mod key_pairing;
pub mod location_format;
pub mod location_source;
pub mod loop_waker;
pub mod primary_scroll;
pub mod route;
pub mod system_preferences;
pub mod text_input;
pub mod window;
pub mod window_command;
mod wire;

pub use access_audit::{AccessAudit, AccessRule, AccessSeverity, AccessViolation};
pub use accessibility::{
    AccessNode, CurrentKind, HasPopup, Modal, NumericValue, Orientation, Positioning, Role,
    SetPosition, SortDirection, ToggleKind,
};
pub use app_ctx::{AppCtx, RedrawWaker};
pub use asset_base::{asset_base, asset_url, set_asset_base};
pub use chord::{
    Chord, ChordError, KeyboardConvention, keyboard_convention, set_keyboard_convention,
};
pub use consumed_keys::{ConsumedKeys, consumes, key_member};
pub use destination::{
    DEFAULT_URI_SCHEMES, DENIED_URI_SCHEMES, Destination, IntoDestination, RouteAddress, Uri,
    UriError, address_of, allow_uri_scheme, anchor, external, in_locale, is_uri_scheme_allowed,
};
pub use dom_code::unmodified_key_of_code;
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
pub use key_pairing::KeyPairing;
pub use location_format::{
    LocationFormat, installed_location_format, location_format, set_location_format,
};
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
pub use text_input::{Ime, TextInput};
pub use window::{
    Cursor, EventHandler, FullscreenMode, MultiSurfacePlatform, Platform, ResizeEdge, SurfaceId,
    Window, WindowConfig, WindowPosition, window_waker,
};
pub use window_command::{
    WindowCommand, WindowCommandContext, push_window_command, take_window_commands,
};
