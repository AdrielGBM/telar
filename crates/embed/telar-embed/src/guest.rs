//! The guest's side of the embedding boundary: what each of the [`EmbedEntries`](crate::EmbedEntries) runs inside the library, every body under [`guest_entry`], every output written through a pointer the host passed in and every rich value read from or lent as bytes.

use std::time::Duration;

use platform_core::guest::{
    CWaker, CallOutcome, FrameCtx, Handle, HostServices, LentBytes, Timestamp, guest_entry,
    host_epoch,
};
use platform_core::{Event, SystemPreferences};
use renderer_core::Color;
use services_core::guest::LentFrames;

use crate::{EmbedInstance, EmbeddedApp};

/// Exports a guest cdylib's [`EmbedEntries`](crate::EmbedEntries), whose `create` builds an instance of the app `$factory` returns, and the [`EMBED_CONTRACT`](crate::EMBED_CONTRACT) its host checks before reading them. `$factory` is any `FnOnce(&[String]) -> Box<dyn EmbeddedApp>`, invoked once per instance with the launch args.
///
/// ```ignore
/// telar_embed::embed!(|args: &[String]| -> Box<dyn telar_embed::EmbeddedApp> { Box::new(MyApp::new(args)) });
/// ```
///
/// One table rather than one symbol per entry, so adding an entry is a field of the table, and a stale `.so` is refused by its stamp rather than by a missing-symbol error naming whichever entry happened to be added last.
#[macro_export]
macro_rules! embed {
    ($factory:expr) => {
        #[doc(hidden)]
        unsafe extern "C" fn __telar_embed_create(
            args: $crate::LentBytes,
            instance: *mut $crate::Handle<$crate::guest::GuestInstance>,
        ) -> $crate::CallOutcome {
            let make = $factory;
            unsafe { $crate::guest::create(args, instance, make) }
        }

        #[doc(hidden)]
        #[unsafe(export_name = "telar_embed_entries")]
        pub static TELAR_EMBED_ENTRIES: $crate::EmbedEntries =
            $crate::EmbedEntries::serving(__telar_embed_create);

        $crate::export_contract_stamp!($crate::EMBED_CONTRACT);
    };
}

/// One instance a library built, behind the handle its host holds, with the frames it lends.
pub struct GuestInstance {
    instance: EmbedInstance,
    frames: LentFrames,
}

/// Builds one instance of the app `make` returns for the launch arguments the host lent, and hands the host a handle on it.
///
/// # Safety
///
/// `args` must be lent for the call, `instance` writable, and the handle written there released only through this library's `release`.
pub unsafe fn create(
    args: LentBytes,
    instance: *mut Handle<GuestInstance>,
    make: impl FnOnce(&[String]) -> Box<dyn EmbeddedApp>,
) -> CallOutcome {
    guest_entry(|| {
        let args: Vec<String> = unsafe { args.read() };
        let built = GuestInstance {
            instance: EmbedInstance::new(make(&args)),
            frames: LentFrames::new(),
        };
        unsafe { instance.write(Handle::new(built)) };
    })
}

/// Fills this copy's stores from the host's table, and has its shaper defer to the host's font database. The table is the host's, kept until it has unloaded this copy.
pub(crate) unsafe extern "C" fn install_services(services: HostServices) -> CallOutcome {
    guest_entry(move || {
        let fonts = unsafe { services_core::guest::install_host_services(services) };
        #[cfg(feature = "shaper")]
        if let Some(fonts) = fonts {
            renderer_text::fonts::defer_to(fonts);
        }
        #[cfg(not(feature = "shaper"))]
        drop(fonts);
    })
}

/// Drops the instance on the side whose code its destructor is.
pub(crate) unsafe extern "C" fn release(instance: Handle<GuestInstance>) -> CallOutcome {
    guest_entry(|| unsafe { instance.release() })
}

pub(crate) unsafe extern "C" fn relayout(
    instance: Handle<GuestInstance>,
    width: f32,
    height: f32,
) -> CallOutcome {
    guest_entry(|| unsafe { instance.get() }.instance.relayout(width, height))
}

pub(crate) unsafe extern "C" fn relayout_dirty(instance: Handle<GuestInstance>) -> CallOutcome {
    guest_entry(|| unsafe { instance.get() }.instance.relayout_dirty())
}

pub(crate) unsafe extern "C" fn paint(
    instance: Handle<GuestInstance>,
    frame: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let guest = unsafe { instance.get() };
        let painted = guest.frames.paint(&guest.instance.paint());
        unsafe { frame.write(painted) };
    })
}

pub(crate) unsafe extern "C" fn pixels(
    instance: Handle<GuestInstance>,
    image: u64,
    pixels: *mut LentBytes,
    found: *mut bool,
) -> CallOutcome {
    guest_entry(|| unsafe { instance.get().frames.lend_pixels(image, pixels, found) })
}

pub(crate) unsafe extern "C" fn reset_frames(instance: Handle<GuestInstance>) -> CallOutcome {
    guest_entry(|| unsafe { instance.get() }.frames.reset())
}

pub(crate) unsafe extern "C" fn generation(
    instance: Handle<GuestInstance>,
    generation: *mut u64,
) -> CallOutcome {
    guest_entry(|| {
        let current = unsafe { instance.get() }.instance.generation();
        unsafe { generation.write(current) };
    })
}

pub(crate) unsafe extern "C" fn on_event(
    instance: Handle<GuestInstance>,
    event: LentBytes,
    handled: *mut bool,
) -> CallOutcome {
    guest_entry(|| {
        let event: Event = unsafe { event.read() };
        let taken = unsafe { instance.get() }.instance.on_event(&event);
        unsafe { handled.write(taken) };
    })
}

pub(crate) unsafe extern "C" fn dispatch_overlays(
    instance: Handle<GuestInstance>,
    event: LentBytes,
    handled: *mut bool,
) -> CallOutcome {
    guest_entry(|| {
        let event: Event = unsafe { event.read() };
        let taken = unsafe { instance.get() }.instance.dispatch_overlays(&event);
        unsafe { handled.write(taken) };
    })
}

pub(crate) unsafe extern "C" fn end_frame(instance: Handle<GuestInstance>) -> CallOutcome {
    guest_entry(|| unsafe { instance.get() }.instance.end_frame())
}

pub(crate) unsafe extern "C" fn motion_tick(
    instance: Handle<GuestInstance>,
    now: Timestamp,
) -> CallOutcome {
    guest_entry(|| {
        let now = host_epoch().instant(now);
        unsafe { instance.get() }.instance.motion_tick(now);
    })
}

pub(crate) unsafe extern "C" fn motion_has_active(
    instance: Handle<GuestInstance>,
    active: *mut bool,
) -> CallOutcome {
    guest_entry(|| {
        let animating = unsafe { instance.get() }.instance.motion_has_active();
        unsafe { active.write(animating) };
    })
}

pub(crate) unsafe extern "C" fn until_next_timer(
    instance: Handle<GuestInstance>,
    after: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let due: Option<Duration> = unsafe { instance.get() }.instance.until_next_timer();
        unsafe { after.write(LentBytes::answer(&due)) };
    })
}

/// The deadline on this side's timer clock, as a timestamp on the host's: the same deadline answers with the same timestamp, which is all the host compares.
pub(crate) unsafe extern "C" fn next_timer_due(
    instance: Handle<GuestInstance>,
    due: *mut Timestamp,
    found: *mut bool,
) -> CallOutcome {
    guest_entry(|| {
        let next = unsafe { instance.get() }.instance.next_timer_due();
        let stamp = next.map(|next| host_epoch().stamp(next));
        unsafe {
            found.write(stamp.is_some());
            due.write(stamp.unwrap_or_default());
        }
    })
}

pub(crate) unsafe extern "C" fn drain_window_commands(
    instance: Handle<GuestInstance>,
    commands: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let drained = unsafe { instance.get() }.instance.drain_window_commands();
        unsafe { commands.write(LentBytes::answer(&drained)) };
    })
}

pub(crate) unsafe extern "C" fn set_system_preferences(
    instance: Handle<GuestInstance>,
    preferences: LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let preferences: SystemPreferences = unsafe { preferences.read() };
        unsafe { instance.get() }
            .instance
            .set_system_preferences(&preferences);
    })
}

pub(crate) unsafe extern "C" fn activate(instance: Handle<GuestInstance>) -> CallOutcome {
    guest_entry(|| unsafe { instance.get() }.instance.activate())
}

pub(crate) unsafe extern "C" fn clear_color(
    instance: Handle<GuestInstance>,
    color: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let clear: Option<Color> = unsafe { instance.get() }.instance.clear_color();
        unsafe { color.write(LentBytes::answer(&clear)) };
    })
}

pub(crate) unsafe extern "C" fn title(
    instance: Handle<GuestInstance>,
    title: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let titled = unsafe { instance.get() }.instance.title();
        unsafe { title.write(LentBytes::answer(&titled)) };
    })
}

pub(crate) unsafe extern "C" fn icon(
    instance: Handle<GuestInstance>,
    icon: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let bytes = unsafe { instance.get() }.instance.icon();
        unsafe { icon.write(LentBytes::answer(&bytes)) };
    })
}

pub(crate) unsafe extern "C" fn id(
    instance: Handle<GuestInstance>,
    id: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let named = unsafe { instance.get() }.instance.id();
        unsafe { id.write(LentBytes::answer(&named)) };
    })
}

pub(crate) unsafe extern "C" fn on_frame(
    instance: Handle<GuestInstance>,
    ctx: *mut FrameCtx,
) -> CallOutcome {
    guest_entry(|| {
        let guest = unsafe { instance.get() };
        unsafe { FrameCtx::enter(ctx, |ctx| guest.instance.on_frame(ctx)) };
    })
}

/// Keeps the reference `waker` is, which this library drops, and with it hands back to the host, when it is replaced or the library unloads: the guest links its own reactive-core, so a task it spawns finishes in a runtime the host's loop cannot see.
pub(crate) extern "C" fn install_task_waker(waker: CWaker) -> CallOutcome {
    guest_entry(move || reactive_core::set_task_waker(move || waker.wake()))
}
