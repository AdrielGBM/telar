//! [`EmbedEntries`]: the one table an embedded guest exports, and the [`EMBED_CONTRACT`] its host checks before reading it.

use platform_core::guest::{
    CWaker, ContractStamp, Entries, FrameCtx, Handle, LentBytes, Timestamp,
};

use crate::guest::{self, GuestInstance};

platform_core::entry_table! {
    /// Every entry of an embedded guest after its lifecycle, each the guest's function of the same name in [`guest`] but `create`, which the [`embed!`](crate::embed) macro writes, building one instance from the launch arguments lent as a `Vec<String>`, and the `report` of what an instance tells the host about the surface it draws: named for the method of the host's it serves. Each returns a `CallOutcome` and writes what it answers through the pointers it is handed.
    pub struct EmbedEntries {
        kind: Embed,
        symbol: "telar_embed_entries",
        guest: guest,
        settings: ["shaper" => cfg!(feature = "shaper")],
        create: fn(LentBytes, *mut Handle<GuestInstance>) as CreateInstance;
        report: GuestInstance;
        release: fn(Handle<GuestInstance>);
        relayout: fn(Handle<GuestInstance>, f32, f32);
        relayout_dirty: fn(Handle<GuestInstance>);
        paint: fn(Handle<GuestInstance>, *mut LentBytes);
        pixels: fn(Handle<GuestInstance>, u64, *mut LentBytes, *mut bool);
        reset_frames: fn(Handle<GuestInstance>);
        generation: fn(Handle<GuestInstance>, *mut u64);
        on_event: fn(Handle<GuestInstance>, LentBytes, *mut bool);
        dispatch_overlays: fn(Handle<GuestInstance>, LentBytes, *mut bool);
        end_frame: fn(Handle<GuestInstance>);
        motion_tick: fn(Handle<GuestInstance>, Timestamp);
        motion_has_active: fn(Handle<GuestInstance>, *mut bool);
        until_next_timer: fn(Handle<GuestInstance>, *mut LentBytes);
        next_timer_due: fn(Handle<GuestInstance>, *mut Timestamp, *mut bool);
        drain_window_commands: fn(Handle<GuestInstance>, *mut LentBytes);
        set_system_preferences: fn(Handle<GuestInstance>, LentBytes);
        activate: fn(Handle<GuestInstance>);
        clear_color: fn(Handle<GuestInstance>, *mut LentBytes);
        title: fn(Handle<GuestInstance>, *mut LentBytes);
        icon: fn(Handle<GuestInstance>, *mut LentBytes);
        id: fn(Handle<GuestInstance>, *mut LentBytes);
        on_frame: fn(Handle<GuestInstance>, *mut FrameCtx);
        install_task_waker: fn(CWaker);
    }
}

/// The stamp of the embedding boundary, which a guest exports and its host refuses it without. Besides the layouts it hashes `shaper`: a guest without it would leave the font database the host serves unused and measure its text in faces other than the ones the host draws it in.
pub const EMBED_CONTRACT: ContractStamp = <EmbedEntries as Entries>::CONTRACT;

#[cfg(test)]
#[path = "entries_test.rs"]
mod tests;
