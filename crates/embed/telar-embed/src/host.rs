//! The host's side of the embedding boundary: [`load_embedded`] and the [`LoadedEmbed`] it hands back, which drives one guest instance through the entries its library exports.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::time::Duration;

use platform_core::guest::{CWaker, FrameCtx, Handle, LentBytes, Timestamp};
use platform_core::{AppCtx, Event, SystemPreferences};
use renderer_core::Color;
use renderer_core::wire::FrameDecoder;
use services_core::guest::GuestLibrary;
use web_time::Instant;
use wire_core::encode;

use crate::guest::GuestInstance;
use crate::{DrawList, EmbedEntries, GuestPanic, WindowCommands};

/// A loaded guest the host drives: one instance its library built, and the library itself, which outlives the instance. `!Send`/`!Sync`: the instance is a foreign reactive runtime, driven only on the UI thread.
///
/// Every call goes through its [`GuestLibrary`], so once the guest has panicked none reaches it again: [`panic`](Self::panic) says what happened and where, and every method answers as an empty guest would — nothing painted, no event taken, no timer waiting. The host shows the report in the guest's place.
pub struct LoadedEmbed {
    instance: Handle<GuestInstance>,
    library: GuestLibrary<EmbedEntries>,
    /// Keeps the payloads and images consecutive frames share.
    frames: RefCell<FrameDecoder>,
    timer_mirror: TimerMirror,
    task_waker_installed: Cell<bool>,
}

/// Loads a guest cdylib and creates one instance from it, its `create` entry handed `args`.
///
/// Each load opens a private copy of the library, so a rebuilt library at the same path never shares statics with one still mapped, a guest that panicked among them. The library is kept mapped for the guest's lifetime. An error names the library and why it could not be driven: one built against another telar, with other features, as a hot-reload library or as no telar guest at all is refused before anything of it runs but its initializers. A panic while the guest is built comes back as the error too, a [`GuestPanic`] the caller can downcast to.
///
/// # Safety
///
/// As [`open_guest`](platform_core::guest::open_guest): `path` must name an embedded guest built from a telar this host trusts. Loading runs its initializers, and once its [`EMBED_CONTRACT`](crate::EMBED_CONTRACT) matches, its [`EmbedEntries`] are called as they are, foreign code in this process.
pub unsafe fn load_embedded(
    path: &Path,
    args: &[String],
) -> Result<LoadedEmbed, Box<dyn std::error::Error>> {
    let (entries, library) = unsafe { platform_core::guest::open_guest::<EmbedEntries>(path) }
        .map_err(|why| format!("{}: {why}", path.display()))?;
    Ok(LoadedEmbed::start(entries, args, Some(library))?)
}

impl LoadedEmbed {
    /// Starts the guest `entries` were copied from, creates one instance through them and seeds it with the host's system preferences; `library` is its mapping, `None` for entries this process serves itself. A guest that panicked by then is unloaded, and its report handed back.
    pub(crate) fn start(
        entries: EmbedEntries,
        args: &[String],
        library: Option<libloading::Library>,
    ) -> Result<Self, GuestPanic> {
        let library = GuestLibrary::start(
            entries,
            library.map(|library| Box::new(library) as Box<dyn std::any::Any>),
            served_fonts(),
        );
        let args = encode(&args.to_vec());
        let instance = library.answer(Handle::NULL, |entries, instance| unsafe {
            (entries.create)(LentBytes::lend(&args), instance)
        });
        let Some(instance) = instance else {
            return Err(panic_of(&library));
        };
        let embed = Self {
            instance,
            library,
            frames: RefCell::default(),
            timer_mirror: TimerMirror::default(),
            task_waker_installed: Cell::new(false),
        };
        // A guest starts with a store of its own that knows nothing, and a host only forwards changes; without this one opened mid-session would draw light on a dark desktop until the user next changed a setting.
        embed.set_system_preferences(&preferences_core::system_preferences());
        match embed.panic() {
            Some(panic) => Err(panic.clone()),
            None => Ok(embed),
        }
    }

    /// The panic that stopped this guest, if one has: what it said and where. Nothing of the guest runs after it.
    pub fn panic(&self) -> Option<&GuestPanic> {
        self.library.guest().panic()
    }

    pub fn relayout(&self, width: f32, height: f32) {
        let instance = self.instance;
        self.library
            .call(|entries| unsafe { (entries.relayout)(instance, width, height) });
    }

    pub fn relayout_dirty(&self) {
        let instance = self.instance;
        self.library
            .call(|entries| unsafe { (entries.relayout_dirty)(instance) });
    }

    /// The guest's frame, decoded into commands of the host's own.
    pub fn paint(&self) -> DrawList {
        let instance = self.instance;
        self.library.read_frame(
            &mut self.frames.borrow_mut(),
            |entries, frame| unsafe { (entries.paint)(instance, frame) },
            |entries, image, pixels, found| unsafe {
                (entries.pixels)(instance, image, pixels, found)
            },
            |entries| unsafe { (entries.reset_frames)(instance) },
        )
    }

    pub fn generation(&self) -> u64 {
        let instance = self.instance;
        self.library
            .answer(0, |entries, generation| unsafe {
                (entries.generation)(instance, generation)
            })
            .unwrap_or_default()
    }

    pub fn on_event(&self, event: &Event) -> bool {
        let instance = self.instance;
        let event = encode(event);
        self.library.ask(|entries, handled| unsafe {
            (entries.on_event)(instance, LentBytes::lend(&event), handled)
        })
    }

    pub fn dispatch_overlays(&self, event: &Event) -> bool {
        let instance = self.instance;
        let event = encode(event);
        self.library.ask(|entries, handled| unsafe {
            (entries.dispatch_overlays)(instance, LentBytes::lend(&event), handled)
        })
    }

    /// Call once per frame the host drove this guest through, after its events. Closes the guest's one-frame keyboard state, which `key_pressed` inside it answers from.
    pub fn end_frame(&self) {
        let instance = self.instance;
        self.library
            .call(|entries| unsafe { (entries.end_frame)(instance) });
    }

    pub fn motion_tick(&self, now: Instant) {
        let instance = self.instance;
        let now = self.library.epoch().stamp(now);
        self.library
            .call(|entries| unsafe { (entries.motion_tick)(instance, now) });
    }

    pub fn motion_has_active(&self) -> bool {
        let instance = self.instance;
        self.library
            .ask(|entries, active| unsafe { (entries.motion_has_active)(instance, active) })
    }

    pub fn drain_window_commands(&self) -> WindowCommands {
        let instance = self.instance;
        self.library
            .read(|entries, commands| unsafe {
                (entries.drain_window_commands)(instance, commands)
            })
            .unwrap_or_default()
    }

    /// Hands the guest a new snapshot of the user's system preferences. [`load_embedded`] seeds it with the host's own; call this from the host's `on_system_preferences` so the guest follows later changes.
    pub fn set_system_preferences(&self, preferences: &SystemPreferences) {
        let instance = self.instance;
        self.library
            .send(preferences, |entries, preferences| unsafe {
                (entries.set_system_preferences)(instance, preferences)
            });
    }

    pub fn activate(&self) {
        let instance = self.instance;
        self.library
            .call(|entries| unsafe { (entries.activate)(instance) });
    }

    pub fn clear_color(&self) -> Option<Color> {
        let instance = self.instance;
        self.library
            .read(|entries, color| unsafe { (entries.clear_color)(instance, color) })
            .flatten()
    }

    pub fn title(&self) -> String {
        let instance = self.instance;
        self.library
            .read(|entries, title| unsafe { (entries.title)(instance, title) })
            .unwrap_or_default()
    }

    pub fn icon(&self) -> Option<Vec<u8>> {
        let instance = self.instance;
        self.library
            .read(|entries, icon| unsafe { (entries.icon)(instance, icon) })
            .flatten()
    }

    pub fn id(&self) -> String {
        let instance = self.instance;
        self.library
            .read(|entries, id| unsafe { (entries.id)(instance, id) })
            .unwrap_or_default()
    }

    /// Call once per frame. Hands the guest the host's loop waker for its tasks the first time there is one, runs the guest's due timers, its finished tasks and its own hook, then keeps the host's loop waking for the guest's next timer.
    pub fn on_frame(&self, ctx: &mut AppCtx) {
        if !self.task_waker_installed.get()
            && let Some(waker) = ctx.redraw_waker()
        {
            let waker = CWaker::new(waker.clone());
            self.library
                .call(move |entries| unsafe { (entries.install_task_waker)(waker) });
            self.task_waker_installed.set(true);
        }
        let instance = self.instance;
        FrameCtx::lend(ctx, |frame| {
            self.library
                .call(|entries| unsafe { (entries.on_frame)(instance, frame) })
        });
        self.timer_mirror
            .follow(self.next_timer_due(), self.until_next_timer());
    }

    /// How long until the guest's earliest timer comes due; `None` when none is waiting. [`on_frame`](Self::on_frame) already keeps the host's loop waking for it.
    pub fn until_next_timer(&self) -> Option<Duration> {
        let instance = self.instance;
        self.library
            .read(|entries, after| unsafe { (entries.until_next_timer)(instance, after) })
            .flatten()
    }

    pub(crate) fn next_timer_due(&self) -> Option<Instant> {
        let instance = self.instance;
        let mut found = false;
        let due = self
            .library
            .answer(Timestamp::default(), |entries, due| unsafe {
                (entries.next_timer_due)(instance, due, &mut found)
            })?;
        found.then(|| self.library.epoch().instant(due))
    }
}

fn panic_of(library: &GuestLibrary<EmbedEntries>) -> GuestPanic {
    library
        .guest()
        .panic()
        .cloned()
        .unwrap_or_else(|| GuestPanic {
            message: "the guest built no instance".to_owned(),
            location: None,
        })
}

/// The guest's earliest timer, held as a timer of the host's own. The host's loop sleeps until the earliest deadline in the host's runtime, and the guest's timers are in a copy of their own it cannot see: this one runs nothing, it only wakes the frame in which the host drives the guest's [`on_frame`](LoadedEmbed::on_frame).
#[derive(Default)]
pub(crate) struct TimerMirror(RefCell<Option<(Instant, reactive_core::Timer)>>);

impl TimerMirror {
    /// Follows the guest's earliest deadline, `due` on its clock and `wait` from now. A deadline already followed is left alone: scheduling it again would wake the loop every frame for a timer that has not moved.
    pub(crate) fn follow(&self, due: Option<Instant>, wait: Option<Duration>) {
        let mut mirrored = self.0.borrow_mut();
        let following = mirrored
            .as_ref()
            .is_some_and(|(at, timer)| Some(*at) == due && timer.is_pending());
        if !following {
            *mirrored = due
                .zip(wait)
                .map(|(due, wait)| (due, reactive_core::run_after(wait, || {})));
        }
    }
}

#[cfg(feature = "shaper")]
fn served_fonts() -> Option<std::sync::Arc<dyn renderer_core::FontHost>> {
    Some(std::sync::Arc::new(renderer_text::fonts::Served))
}

#[cfg(not(feature = "shaper"))]
fn served_fonts() -> Option<std::sync::Arc<dyn renderer_core::FontHost>> {
    None
}

impl Drop for LoadedEmbed {
    // A guest that panicked keeps its instance: releasing it would run that guest's code again. Its library is unloaded after, as the field drops.
    fn drop(&mut self) {
        let instance = self.instance;
        self.library
            .call(|entries| unsafe { (entries.release)(instance) });
    }
}
