//! [`HotEntries`]: the one table a hot-reload library exports, and the [`HOT_CONTRACT`] its host checks before reading it.

use platform_core::guest::{
    CWaker, ContractStamp, Entries, FrameCtx, Handle, LentBytes, Timestamp,
};

use super::guest::{self, GuestApp};
use crate::tree::HotTree;

platform_core::entry_table! {
    /// Every entry of a hot-reload library after its lifecycle, each the guest's function of the same name in [`guest`] but `create`, which the `app!` macro writes: named for the method of the host's it serves, those on a mounted tree prefixed `tree_`. Each returns a `CallOutcome` and writes what it answers through the pointers it is handed.
    pub struct HotEntries {
        kind: HotReload,
        symbol: "telar_hot_entries",
        guest: guest,
        settings: ["shaper" => cfg!(feature = "shaper")],
        create: fn(*mut Handle<GuestApp>) as CreateApp;
        release: fn(Handle<GuestApp>);
        window_config: fn(Handle<GuestApp>, *mut LentBytes);
        clear_color: fn(Handle<GuestApp>, *mut LentBytes);
        on_frame: fn(Handle<GuestApp>, *mut FrameCtx);
        mount: fn(Handle<GuestApp>, *mut Handle<HotTree>);
        tree_release: fn(Handle<HotTree>);
        tree_on_event: fn(Handle<HotTree>, LentBytes, *mut bool);
        tree_on_covered_event: fn(Handle<HotTree>, LentBytes);
        tree_end_frame: fn(Handle<HotTree>);
        tree_paint: fn(Handle<HotTree>, *mut LentBytes);
        tree_pixels: fn(Handle<HotTree>, u64, *mut LentBytes, *mut bool);
        tree_reset_frames: fn(Handle<HotTree>);
        tree_is_dirty: fn(Handle<HotTree>, *mut bool);
        tree_generation: fn(Handle<HotTree>, *mut u64);
        tree_walk: fn(Handle<HotTree>, *mut LentBytes);
        tree_access_audit: fn(Handle<HotTree>, LentBytes, *mut LentBytes);
        hot_snapshot: fn(*mut LentBytes);
        hot_restore: fn(LentBytes);
        motion_tick: fn(Timestamp);
        motion_has_active: fn(*mut bool);
        motion_has_continuous: fn(*mut bool);
        begin_batch: fn();
        end_batch: fn();
        relayout: fn();
        dispatch_overlays: fn(LentBytes, *mut bool);
        set_system_preferences: fn(LentBytes);
        set_surface_size: fn(f32, f32);
        set_safe_area_insets: fn(f32, f32, f32, f32);
        set_surface_maximized: fn(bool);
        drain_window_commands: fn(*mut LentBytes);
        text_input: fn(*mut LentBytes);
        set_location_history: fn(LentBytes);
        open_title: fn(LentBytes);
        open_font_family: fn(LentBytes);
        navigate_back: fn(*mut bool);
        drain_tasks: fn();
        fire_timers: fn();
        until_next_timer: fn(*mut LentBytes);
        install_task_waker: fn(CWaker);
    }
}

/// The stamp of the hot-reload boundary, which a library exports and its host refuses it without. Besides the layouts it hashes `shaper`: a library without it would leave the font database the host serves unused and measure its text in faces other than the ones the host draws it in.
pub const HOT_CONTRACT: ContractStamp = <HotEntries as Entries>::CONTRACT;

#[cfg(test)]
#[path = "entries_test.rs"]
mod tests;
