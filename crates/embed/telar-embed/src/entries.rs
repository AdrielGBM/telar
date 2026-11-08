//! [`EmbedEntries`]: the one table an embedded guest exports, and the [`EMBED_CONTRACT`] its host checks before reading it.

use std::time::Duration;

use platform_core::guest::{
    CWaker, ContractStamp, Entries, FrameCtx, Handle, LentBytes, Timestamp,
};
use platform_core::{Event, SystemPreferences};
use renderer_core::Color;
use renderer_core::wire::{DrawList, Pixels};

use crate::WindowCommands;
use crate::guest::{self, GuestInstance};

platform_core::entry_table! {
    /// Every entry of an embedded guest after its lifecycle, each the guest's function of the same name in [`guest`] but `create`, which the [`embed!`](crate::embed) macro writes, building one instance from the launch arguments lent as a `Vec<String>`, and the `report` of what an instance tells the host about the surface it draws: named for the method of the host's it serves. Each returns a `CallOutcome` and writes what it answers through the pointers it is handed.
    pub struct EmbedEntries {
        kind: Embed,
        symbol: "telar_embed_entries",
        guest: guest,
        settings: [],
        services: services_core::guest::UpperTable,
        create: fn(LentBytes<Vec<String>>, *mut Handle<GuestInstance>) as CreateInstance;
        report: GuestInstance;
        release: fn(Handle<GuestInstance>);
        relayout: fn(Handle<GuestInstance>, f32, f32);
        relayout_dirty: fn(Handle<GuestInstance>);
        paint: fn(Handle<GuestInstance>, *mut LentBytes<DrawList>);
        pixels: fn(Handle<GuestInstance>, u64, *mut LentBytes<Pixels>, *mut bool);
        reset_frames: fn(Handle<GuestInstance>);
        generation: fn(Handle<GuestInstance>, *mut u64);
        on_event: fn(Handle<GuestInstance>, LentBytes<Event>, *mut bool);
        dispatch_overlays: fn(Handle<GuestInstance>, LentBytes<Event>, *mut bool);
        end_frame: fn(Handle<GuestInstance>);
        motion_tick: fn(Handle<GuestInstance>, Timestamp);
        motion_has_active: fn(Handle<GuestInstance>, *mut bool);
        until_next_timer: fn(Handle<GuestInstance>, *mut LentBytes<Option<Duration>>);
        next_timer_due: fn(Handle<GuestInstance>, *mut Timestamp, *mut bool);
        drain_window_commands: fn(Handle<GuestInstance>, *mut LentBytes<WindowCommands>);
        set_system_preferences: fn(Handle<GuestInstance>, LentBytes<SystemPreferences>);
        activate: fn(Handle<GuestInstance>);
        clear_color: fn(Handle<GuestInstance>, *mut LentBytes<Option<Color>>);
        title: fn(Handle<GuestInstance>, *mut LentBytes<String>);
        icon: fn(Handle<GuestInstance>, *mut LentBytes<Option<Vec<u8>>>);
        id: fn(Handle<GuestInstance>, *mut LentBytes<String>);
        on_frame: fn(Handle<GuestInstance>, *mut FrameCtx);
        install_task_waker: fn(CWaker);
    }
}

/// The stamp of the embedding boundary, which a guest exports and its host refuses it without. It hashes the layouts alone: whether the guest sets text in the host's faces is settled when it runs, since the host always serves its font database and a guest's shaper defers to it once it first needs a face.
pub const EMBED_CONTRACT: ContractStamp = <EmbedEntries as Entries>::CONTRACT;

#[cfg(test)]
#[path = "entries.test.rs"]
mod tests;
