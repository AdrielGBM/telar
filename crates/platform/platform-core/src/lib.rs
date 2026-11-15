//! The seam between Telar and whatever opens its windows.
//!
//! A backend implements [`Platform`] and [`Window`] against these types; everything above this crate speaks only in them, so a frontend can be swapped without touching the UI.

#![warn(rustdoc::broken_intra_doc_links)]

// Named by the macros a boundary declares its tables with, which expand in other crates.
#[doc(hidden)]
pub use wire_core;
pub mod access_audit;
pub mod accessibility;
pub mod app_ctx;
pub mod asset_base;
pub mod chord;
pub mod consumed_keys;
pub mod destination;
pub mod dom_code;
pub mod drag_drop;
pub mod error;
pub mod event;
pub mod event_sink;
pub mod focus_boundary;
pub mod guest;
pub mod history;
pub mod key_pairing;
pub mod location_format;
pub mod location_source;
mod logical_extent;
pub mod loop_waker;
pub mod primary_scroll;
pub mod route;
pub mod surface_conventions;
pub mod system_preferences;
pub mod text_input;
pub mod window;
pub mod window_command;
mod wire;

pub use access_audit::{AccessAudit, AccessRule, AccessSeverity, AccessViolation};
pub use accessibility::{
    AccessAction, AccessId, AccessNode, ActionSupport, CurrentKind, FocusId, HasPopup,
    HeadingLevel, Link, Live, Modal, NumericValue, NumericValueError, Orientation, Positioning,
    Role, SetPosition, SortDirection, TextSelection, Toggle, ToggleKind, Value,
};
pub use app_ctx::{AppCtx, RedrawWaker};
pub use asset_base::{asset_base, asset_url, set_asset_base};
pub use chord::{
    Chord, ChordError, KeyboardConvention, KeyboardConventionContext, keyboard_convention,
    set_keyboard_convention,
};
pub use consumed_keys::{ConsumedKeys, consumes, key_member};
pub use destination::{
    DEFAULT_URI_SCHEMES, DENIED_URI_SCHEMES, Destination, IntoDestination, RouteAddress, Uri,
    UriError, address_of, allow_uri_scheme, anchor, external, in_locale, is_uri_scheme_allowed,
};
pub use dom_code::unmodified_key_of_code;
pub use drag_drop::{
    DragActions, DragData, DragIcon, DragOffer, DragOutcome, DropAction, DropAnswer, DroppedData,
    MimeType, MimeTypeError, OutgoingDrag, Representation,
};
pub use error::PlatformError;
pub use event::{
    Event, Key, ModifiersState, NamedKey, PointerButton, PointerSource, ScrollDelta, TAP_SLOP,
};
pub use event_sink::{post_event, set_event_sink};
pub use focus_boundary::{FocusBoundary, TabLeaves};
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
pub use logical_extent::logical_extent;
pub use loop_waker::{loop_waker, set_loop_waker};
pub use primary_scroll::{
    PrimaryScroll, PrimaryScrollClaim, claim_primary_scroll, primary_scroll_offset,
    scroll_primary_to,
};
pub use route::Route;
pub use semantics_core::{ElementId, LocaleTag, LocaleTagError, Location, SurfaceKey, locale_tag};
pub use surface_conventions::SurfaceConventions;
pub use system_preferences::{
    ColorScheme, SystemPreferences, TextScale, UnknownColorScheme, locales_from_env,
    platform_locales, posix_locale_to_bcp47, system_locales_from_env,
};
pub use text_input::{Ime, TextInput};
pub use window::{
    Cursor, EventHandler, FullscreenMode, MultiSurfacePlatform, Platform, ResizeEdge, SurfaceId,
    UnknownWindowOption, Window, WindowConfig, WindowPosition, window_waker,
};
pub use window_command::{
    WindowCommand, WindowCommandContext, push_window_command, take_window_commands,
};
