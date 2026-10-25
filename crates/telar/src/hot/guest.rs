//! The app's side of the hot-reload boundary: what each of the [`HotEntries`](super::HotEntries) runs inside the library, every body under [`guest_entry`], every output written through a pointer the host passed in and every rich value read from or lent as bytes.

use std::cell::RefCell;
use std::mem::ManuallyDrop;

use platform_core::guest::{
    CWaker, CallOutcome, FrameCtx, Handle, HostServices, LentBytes, Timestamp, guest_entry,
    host_epoch,
};
use platform_core::{Event, Location, SystemPreferences};
use reactive_core::OpenBatch;
use renderer_core::Color;

use crate::app::App;
use crate::tree::HotTree;

/// What a hot-reload library exports, as the `app!` macro writes it: its [`HotEntries`](crate::hot::HotEntries), whose `create` builds the app `make` returns, and the [`HOT_CONTRACT`](crate::hot::HOT_CONTRACT) its host checks before reading them.
#[doc(hidden)]
#[macro_export]
macro_rules! __hot_guest {
    ($make:expr) => {
        #[doc(hidden)]
        unsafe extern "C" fn __telar_hot_create(
            app: *mut $crate::hot::Handle<$crate::hot::guest::GuestApp>,
        ) -> $crate::hot::CallOutcome {
            unsafe { $crate::hot::guest::create(app, $make) }
        }

        #[doc(hidden)]
        #[unsafe(export_name = "telar_hot_entries")]
        pub static TELAR_HOT_ENTRIES: $crate::hot::HotEntries =
            $crate::hot::HotEntries::serving(__telar_hot_create);

        $crate::hot::export_contract_stamp!($crate::hot::HOT_CONTRACT);
    };
}

/// The app a library built, behind the handle its host holds.
pub struct GuestApp(Box<dyn App>);

reactive_core::dropless_thread_local! {
    // Never dropped, which costs nothing: `OpenBatch` is zero-sized, so the `Vec` never allocates.
    static HOST_BATCHES: RefCell<ManuallyDrop<Vec<OpenBatch>>> = const { RefCell::new(ManuallyDrop::new(Vec::new())) };
}

/// Builds the app the library serves, with the setup its `app!` invocation names, and hands the host a handle on it.
///
/// # Safety
///
/// `app` must be writable, and the handle written there released only through this library's `release`.
pub unsafe fn create(
    app: *mut Handle<GuestApp>,
    make: impl FnOnce() -> Box<dyn App>,
) -> CallOutcome {
    guest_entry(|| {
        let made = GuestApp(make());
        unsafe { app.write(Handle::new(made)) };
    })
}

/// Fills this copy's stores from the host's table, and has its shaper defer to the host's font database. The table is the host's, kept until it has unloaded this copy.
pub(super) unsafe extern "C" fn install_services(services: HostServices) -> CallOutcome {
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

/// Drops the app on the side whose code its destructor is.
pub(super) unsafe extern "C" fn release(app: Handle<GuestApp>) -> CallOutcome {
    guest_entry(|| unsafe { app.release() })
}

pub(super) unsafe extern "C" fn window_config(
    app: Handle<GuestApp>,
    config: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let configured = unsafe { app.get() }.0.window_config();
        unsafe { config.write(LentBytes::answer(&configured)) };
    })
}

pub(super) unsafe extern "C" fn clear_color(
    app: Handle<GuestApp>,
    color: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let clear: Option<Color> = unsafe { app.get() }.0.clear_color();
        unsafe { color.write(LentBytes::answer(&clear)) };
    })
}

pub(super) unsafe extern "C" fn on_frame(app: Handle<GuestApp>, ctx: *mut FrameCtx) -> CallOutcome {
    guest_entry(|| {
        let app = unsafe { app.get() };
        unsafe { FrameCtx::enter(ctx, |ctx| app.0.on_frame(ctx)) };
    })
}

/// Mounted here, where the app's signals live, so the tree's segment effects subscribe in the same reactive runtime as the signals they read.
pub(super) unsafe extern "C" fn mount(
    app: Handle<GuestApp>,
    tree: *mut Handle<HotTree>,
) -> CallOutcome {
    guest_entry(|| {
        let mounted = HotTree::mount(unsafe { app.get() }.0.as_ref());
        unsafe { tree.write(Handle::new(mounted)) };
    })
}

pub(super) unsafe extern "C" fn tree_release(tree: Handle<HotTree>) -> CallOutcome {
    guest_entry(|| unsafe { tree.release() })
}

pub(super) unsafe extern "C" fn tree_on_event(
    tree: Handle<HotTree>,
    event: LentBytes,
    handled: *mut bool,
) -> CallOutcome {
    guest_entry(|| {
        let event: Event = unsafe { event.read() };
        let taken = unsafe { tree.get() }.on_event(&event);
        unsafe { handled.write(taken) };
    })
}

pub(super) unsafe extern "C" fn tree_on_covered_event(
    tree: Handle<HotTree>,
    event: LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let event: Event = unsafe { event.read() };
        unsafe { tree.get() }.on_covered_event(&event);
    })
}

pub(super) unsafe extern "C" fn tree_end_frame(tree: Handle<HotTree>) -> CallOutcome {
    guest_entry(|| unsafe { tree.get() }.end_frame())
}

pub(super) unsafe extern "C" fn tree_paint(
    tree: Handle<HotTree>,
    frame: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let painted = unsafe { tree.get() }.paint();
        unsafe { frame.write(painted) };
    })
}

pub(super) unsafe extern "C" fn tree_pixels(
    tree: Handle<HotTree>,
    image: u64,
    pixels: *mut LentBytes,
    found: *mut bool,
) -> CallOutcome {
    guest_entry(|| unsafe { tree.get().lend_pixels(image, pixels, found) })
}

pub(super) unsafe extern "C" fn tree_reset_frames(tree: Handle<HotTree>) -> CallOutcome {
    guest_entry(|| unsafe { tree.get() }.reset_frames())
}

pub(super) unsafe extern "C" fn tree_is_dirty(
    tree: Handle<HotTree>,
    dirty: *mut bool,
) -> CallOutcome {
    guest_entry(|| {
        let is_dirty = unsafe { tree.get() }.is_dirty();
        unsafe { dirty.write(is_dirty) };
    })
}

pub(super) unsafe extern "C" fn tree_generation(
    tree: Handle<HotTree>,
    generation: *mut u64,
) -> CallOutcome {
    guest_entry(|| {
        let current = unsafe { tree.get() }.generation();
        unsafe { generation.write(current) };
    })
}

pub(super) unsafe extern "C" fn tree_walk(
    tree: Handle<HotTree>,
    nodes: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let walked = unsafe { tree.get() }.walk();
        unsafe { nodes.write(LentBytes::answer(&walked)) };
    })
}

/// What a reader is told and what the checks find, for the host's inspector, read from this library's registries, which the host's copies of them never see.
pub(super) unsafe extern "C" fn tree_access_audit(
    tree: Handle<HotTree>,
    background: LentBytes,
    audit: *mut LentBytes,
) -> CallOutcome {
    guest_entry(|| {
        let background: Option<Color> = unsafe { background.read() };
        let audited = unsafe { tree.get() }.access_audit(background);
        unsafe { audit.write(LentBytes::answer(&audited)) };
    })
}

pub(super) unsafe extern "C" fn hot_snapshot(blob: *mut LentBytes) -> CallOutcome {
    guest_entry(|| {
        let taken = crate::hot_snapshot_json();
        unsafe { blob.write(LentBytes::answer(&taken)) };
    })
}

pub(super) unsafe extern "C" fn hot_restore(blob: LentBytes) -> CallOutcome {
    guest_entry(|| {
        let blob: String = unsafe { blob.read() };
        crate::hot_restore_json(&blob);
    })
}

/// The `Animated` values live in this library's motion registry, so the host ticks this one rather than its own.
pub(super) extern "C" fn motion_tick(now: Timestamp) -> CallOutcome {
    guest_entry(|| crate::motion::tick(host_epoch().instant(now)))
}

pub(super) unsafe extern "C" fn motion_has_active(active: *mut bool) -> CallOutcome {
    guest_entry(|| unsafe { active.write(crate::motion::has_active()) })
}

pub(super) unsafe extern "C" fn motion_has_continuous(continuous: *mut bool) -> CallOutcome {
    guest_entry(|| unsafe { continuous.write(crate::motion::has_continuous()) })
}

/// The host's batch cannot reach this library's reactive runtime: without this one a handler's write flushes mid-dispatch, and a segment loses its subscriptions while its widget is borrowed.
pub(super) extern "C" fn begin_batch() -> CallOutcome {
    guest_entry(|| HOST_BATCHES.with(|held| held.borrow_mut().push(OpenBatch::open())))
}

pub(super) extern "C" fn end_batch() -> CallOutcome {
    guest_entry(|| {
        if let Some(batch) = HOST_BATCHES.with(|held| held.borrow_mut().pop()) {
            batch.close();
        }
    })
}

pub(super) extern "C" fn relayout() -> CallOutcome {
    guest_entry(crate::relayout_if_dirty)
}

pub(super) unsafe extern "C" fn dispatch_overlays(
    event: LentBytes,
    handled: *mut bool,
) -> CallOutcome {
    guest_entry(|| {
        let event: Event = unsafe { event.read() };
        unsafe { handled.write(crate::dispatch_overlays(&event)) };
    })
}

pub(super) unsafe extern "C" fn set_system_preferences(preferences: LentBytes) -> CallOutcome {
    guest_entry(|| {
        let preferences: SystemPreferences = unsafe { preferences.read() };
        crate::set_system_preferences(preferences);
    })
}

pub(super) extern "C" fn set_surface_size(width: f32, height: f32) -> CallOutcome {
    guest_entry(|| crate::set_surface_size(crate::Size::new(width, height)))
}

pub(super) extern "C" fn set_safe_area_insets(
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
) -> CallOutcome {
    guest_entry(|| crate::set_safe_area_insets(crate::Insets::new(top, right, bottom, left)))
}

pub(super) extern "C" fn set_surface_maximized(maximized: bool) -> CallOutcome {
    guest_entry(|| crate::set_surface_maximized(maximized))
}

pub(super) unsafe extern "C" fn drain_window_commands(commands: *mut LentBytes) -> CallOutcome {
    guest_entry(|| {
        let drained = crate::take_window_commands();
        unsafe { commands.write(LentBytes::answer(&drained)) };
    })
}

pub(super) unsafe extern "C" fn set_location_history(history: LentBytes) -> CallOutcome {
    guest_entry(|| {
        let history: Vec<Location> = unsafe { history.read() };
        crate::receive_location_history(history);
    })
}

pub(super) unsafe extern "C" fn open_title(title: LentBytes) -> CallOutcome {
    guest_entry(|| {
        let (app, showing): (String, String) = unsafe { title.read() };
        crate::open_surface_title(&app, &showing);
    })
}

pub(super) unsafe extern "C" fn open_font_family(family: LentBytes) -> CallOutcome {
    guest_entry(|| {
        let family: Option<String> = unsafe { family.read() };
        crate::open_surface_font_family(family.as_deref().map(crate::FontFamily::from));
    })
}

pub(super) unsafe extern "C" fn navigate_back(handled: *mut bool) -> CallOutcome {
    guest_entry(|| unsafe { handled.write(crate::navigate_back()) })
}

pub(super) extern "C" fn drain_tasks() -> CallOutcome {
    guest_entry(crate::drain_tasks)
}

pub(super) extern "C" fn fire_timers() -> CallOutcome {
    guest_entry(crate::fire_timers)
}

pub(super) unsafe extern "C" fn until_next_timer(after: *mut LentBytes) -> CallOutcome {
    guest_entry(|| {
        let due = crate::until_next_timer();
        unsafe { after.write(LentBytes::answer(&due)) };
    })
}

/// Keeps the reference `waker` is, which this library drops, and with it hands back to the host, when it is replaced or the library unloads.
pub(super) extern "C" fn install_task_waker(waker: CWaker) -> CallOutcome {
    guest_entry(move || crate::set_task_waker(move || waker.wake()))
}
