//! The app's side of the hot-reload boundary: what each export runs inside the dylib, every body under [`guest_call`].
//!
//! The list of exports is written once, in [`__hot_guest_api`](crate::__hot_guest_api): the `app!` macro turns it into the dylib's `#[no_mangle]` symbols, and the host into the table it resolves them into, so a name or a signature cannot drift between the two.

use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::time::{Duration, Instant};

use platform_core::{
    AccessAudit, AppCtx, Event, Location, RedrawWaker, SystemPreferences, WindowCommand,
    WindowConfig,
};
use renderer_core::{Color, DrawCommand};
use ui_tree::SegmentNodeInfo;

use super::HostServices;
use crate::app::App;
use crate::tree::HotTree;
use platform_core::guest::{GuestResult, guest_call};
pub use platform_core::guest::{loaded, quiesce, unloading};
use reactive_core::OpenBatch;

/// Every export the host resolves, as `field => symbol(arguments) -> output`, handed to the macro the caller names. Each runs [`guest`](crate::hot::guest)'s function of the same field name and returns a [`GuestResult`] of its output.
#[doc(hidden)]
#[macro_export]
macro_rules! __hot_guest_api {
    ($($then:tt)+) => {
        $($then)+! {
            snapshot => _rsx_hot_snapshot() -> ::std::string::String;
            restore => _rsx_hot_restore(blob: &str) -> ();
            app_window_config => _rsx_hot_app_window_config(app: &dyn $crate::App) -> ::std::option::Option<$crate::WindowConfig>;
            app_clear_color => _rsx_hot_app_clear_color(app: &dyn $crate::App) -> ::std::option::Option<$crate::Color>;
            app_on_frame => _rsx_hot_app_on_frame(app: &mut dyn $crate::App, ctx: &mut $crate::AppCtx<'_>) -> ();
            app_release => _rsx_hot_app_release(app: ::std::boxed::Box<dyn $crate::App>) -> ();
            tree_mount => _rsx_hot_tree_mount(app: &dyn $crate::App) -> *mut $crate::HotTree;
            tree_release => _rsx_hot_tree_release(tree: *mut $crate::HotTree) -> ();
            tree_on_event => _rsx_hot_tree_on_event(tree: *mut $crate::HotTree, event: &$crate::Event) -> bool;
            tree_on_covered_event => _rsx_hot_tree_on_covered_event(tree: *mut $crate::HotTree, event: &$crate::Event) -> ();
            tree_end_frame => _rsx_hot_tree_end_frame(tree: *mut $crate::HotTree) -> ();
            tree_paint => _rsx_hot_tree_paint(tree: *mut $crate::HotTree) -> ::std::vec::Vec<$crate::DrawCommand>;
            tree_dirty => _rsx_hot_tree_dirty(tree: *mut $crate::HotTree) -> bool;
            tree_generation => _rsx_hot_tree_generation(tree: *mut $crate::HotTree) -> u64;
            tree_walk => _rsx_hot_tree_walk(tree: *mut $crate::HotTree) -> ::std::vec::Vec<$crate::SegmentNodeInfo>;
            access_snapshot => _rsx_hot_access_snapshot(commands: &[$crate::DrawCommand], background: ::std::option::Option<$crate::Color>) -> $crate::AccessAudit;
            motion_tick => _rsx_hot_motion_tick(now: ::std::time::Instant) -> ();
            motion_active => _rsx_hot_motion_active() -> bool;
            motion_continuous => _rsx_hot_motion_continuous() -> bool;
            begin_batch => _rsx_hot_begin_batch() -> ();
            end_batch => _rsx_hot_end_batch() -> ();
            relayout => _rsx_hot_relayout() -> ();
            dispatch_overlays => _rsx_hot_dispatch_overlays(event: &$crate::Event) -> bool;
            set_system_preferences => _rsx_hot_set_system_preferences(preferences: &$crate::SystemPreferences) -> ();
            set_surface_size => _rsx_hot_set_surface_size(width: f32, height: f32) -> ();
            set_safe_area_insets => _rsx_hot_set_safe_area_insets(top: f32, right: f32, bottom: f32, left: f32) -> ();
            set_surface_maximized => _rsx_hot_set_surface_maximized(maximized: bool) -> ();
            drain_window_commands => _rsx_hot_drain_window_commands() -> ::std::vec::Vec<$crate::WindowCommand>;
            text_input => _rsx_hot_text_input() -> ::std::option::Option<$crate::TextInput>;
            set_location_history => _rsx_hot_set_location_history(history: &[$crate::Location]) -> ();
            open_title => _rsx_hot_open_title(app: &str, showing: &str) -> ();
            open_font_family => _rsx_hot_open_font_family(family: ::std::option::Option<&str>) -> ();
            navigate_back => _rsx_hot_navigate_back() -> bool;
            drain_tasks => _rsx_hot_drain_tasks() -> ();
            fire_timers => _rsx_hot_fire_timers() -> ();
            until_next_timer => _rsx_hot_until_next_timer() -> ::std::option::Option<::std::time::Duration>;
            install_task_waker => _rsx_hot_install_task_waker(waker: $crate::RedrawWaker) -> ();
            install_services => _rsx_hot_install_services(services: $crate::hot::HostServices) -> ();
            loaded => _rsx_hot_loaded() -> ();
            unloading => _rsx_hot_unloading() -> ();
            quiesce => _rsx_hot_quiesce() -> ();
        }
    };
}

/// The dylib's `#[no_mangle]` symbols, from [`__hot_guest_api`](crate::__hot_guest_api), plus the three the host reads before it resolves the rest: the ABI it was built for, the layout fingerprint of what crosses, and the app factory the `app!` macro writes itself.
#[doc(hidden)]
#[macro_export]
macro_rules! __hot_guest_exports {
    ($($field:ident => $symbol:ident($($arg:ident: $ty:ty),*) -> $output:ty;)*) => {
        $(
            #[unsafe(no_mangle)]
            #[allow(unused_unsafe)]
            pub unsafe extern "Rust" fn $symbol($($arg: $ty),*) -> $crate::hot::GuestResult<$output> {
                unsafe { $crate::hot::guest::$field($($arg),*) }
            }
        )*

        #[unsafe(no_mangle)]
        pub extern "Rust" fn _rsx_hot_abi() -> u32 {
            $crate::hot::GUEST_ABI
        }

        #[unsafe(no_mangle)]
        pub extern "Rust" fn _rsx_hot_layout() -> u64 {
            $crate::hot::GUEST_LAYOUT.0
        }
    };
}

/// Builds the app the dylib serves, with the setup its `app!` invocation names.
pub fn create_app(make: impl FnOnce() -> Box<dyn App>) -> GuestResult<Box<dyn App>> {
    guest_call(make)
}

pub fn snapshot() -> GuestResult<String> {
    guest_call(crate::hot_snapshot_json)
}

pub fn restore(blob: &str) -> GuestResult<()> {
    guest_call(|| crate::hot_restore_json(blob))
}

pub fn app_window_config(app: &dyn App) -> GuestResult<Option<WindowConfig>> {
    guest_call(|| app.window_config())
}

pub fn app_clear_color(app: &dyn App) -> GuestResult<Option<Color>> {
    guest_call(|| app.clear_color())
}

pub fn app_on_frame(app: &mut dyn App, ctx: &mut AppCtx<'_>) -> GuestResult<()> {
    guest_call(|| app.on_frame(ctx))
}

/// Drops the app on the side whose code its destructor is.
pub fn app_release(app: Box<dyn App>) -> GuestResult<()> {
    guest_call(move || drop(app))
}

/// Mounted here, where the app's signals live, so the tree's segment effects subscribe in the same reactive runtime as the signals they read.
pub fn tree_mount(app: &dyn App) -> GuestResult<*mut HotTree> {
    guest_call(|| HotTree::mount(app))
}

/// # Safety `tree` must be a live pointer from [`tree_mount`], not yet released.
pub unsafe fn tree_release(tree: *mut HotTree) -> GuestResult<()> {
    guest_call(|| unsafe { HotTree::release(tree) })
}

/// # Safety `tree` must be a live pointer from [`tree_mount`].
pub unsafe fn tree_on_event(tree: *mut HotTree, event: &Event) -> GuestResult<bool> {
    guest_call(|| unsafe { HotTree::on_event(tree, event) })
}

/// # Safety `tree` must be a live pointer from [`tree_mount`].
pub unsafe fn tree_on_covered_event(tree: *mut HotTree, event: &Event) -> GuestResult<()> {
    guest_call(|| unsafe { HotTree::on_covered_event(tree, event) })
}

/// # Safety `tree` must be a live pointer from [`tree_mount`].
pub unsafe fn tree_end_frame(tree: *mut HotTree) -> GuestResult<()> {
    guest_call(|| unsafe { HotTree::end_frame(tree) })
}

/// # Safety `tree` must be a live pointer from [`tree_mount`].
pub unsafe fn tree_paint(tree: *mut HotTree) -> GuestResult<Vec<DrawCommand>> {
    guest_call(|| unsafe { HotTree::paint(tree) })
}

/// # Safety `tree` must be a live pointer from [`tree_mount`].
pub unsafe fn tree_dirty(tree: *mut HotTree) -> GuestResult<bool> {
    guest_call(|| unsafe { HotTree::is_dirty(tree) })
}

/// # Safety `tree` must be a live pointer from [`tree_mount`].
pub unsafe fn tree_generation(tree: *mut HotTree) -> GuestResult<u64> {
    guest_call(|| unsafe { HotTree::generation(tree) })
}

/// # Safety `tree` must be a live pointer from [`tree_mount`].
pub unsafe fn tree_walk(tree: *mut HotTree) -> GuestResult<Vec<SegmentNodeInfo>> {
    guest_call(|| unsafe { HotTree::walk(tree) })
}

/// What a reader is told and what the checks find, read from the dylib's registries, which the host's copies of them never see.
pub fn access_snapshot(
    commands: &[DrawCommand],
    background: Option<Color>,
) -> GuestResult<AccessAudit> {
    guest_call(|| crate::inspect::audit(commands, background))
}

/// The `Animated` values live in the dylib's motion registry, so the host ticks this one rather than its own.
pub fn motion_tick(now: Instant) -> GuestResult<()> {
    guest_call(|| crate::motion::tick(now))
}

pub fn motion_active() -> GuestResult<bool> {
    guest_call(crate::motion::has_active)
}

pub fn motion_continuous() -> GuestResult<bool> {
    guest_call(crate::motion::has_continuous)
}

reactive_core::dropless_thread_local! {
    // Never dropped, which costs nothing: `OpenBatch` is zero-sized, so the `Vec` never allocates.
    static HOST_BATCHES: RefCell<ManuallyDrop<Vec<OpenBatch>>> = const { RefCell::new(ManuallyDrop::new(Vec::new())) };
}

/// The host's batch cannot reach the dylib's reactive runtime: without this one a handler's write flushes mid-dispatch, and a segment loses its subscriptions while its widget is borrowed.
pub fn begin_batch() -> GuestResult<()> {
    guest_call(|| HOST_BATCHES.with(|held| held.borrow_mut().push(OpenBatch::open())))
}

pub fn end_batch() -> GuestResult<()> {
    guest_call(|| {
        if let Some(batch) = HOST_BATCHES.with(|held| held.borrow_mut().pop()) {
            batch.close();
        }
    })
}

pub fn relayout() -> GuestResult<()> {
    guest_call(crate::relayout_if_dirty)
}

pub fn dispatch_overlays(event: &Event) -> GuestResult<bool> {
    guest_call(|| crate::dispatch_overlays(event))
}

pub fn set_system_preferences(preferences: &SystemPreferences) -> GuestResult<()> {
    guest_call(|| crate::set_system_preferences(preferences.clone()))
}

pub fn set_surface_size(width: f32, height: f32) -> GuestResult<()> {
    guest_call(|| crate::set_surface_size(crate::Size::new(width, height)))
}

pub fn set_safe_area_insets(top: f32, right: f32, bottom: f32, left: f32) -> GuestResult<()> {
    guest_call(|| crate::set_safe_area_insets(crate::Insets::new(top, right, bottom, left)))
}

pub fn set_surface_maximized(maximized: bool) -> GuestResult<()> {
    guest_call(|| crate::set_surface_maximized(maximized))
}

pub fn drain_window_commands() -> GuestResult<Vec<WindowCommand>> {
    guest_call(crate::take_window_commands)
}

pub fn text_input() -> GuestResult<Option<platform_core::TextInput>> {
    guest_call(ui_core::text_input)
}

pub fn set_location_history(history: &[Location]) -> GuestResult<()> {
    guest_call(|| crate::receive_location_history(history.to_vec()))
}

pub fn open_title(app: &str, showing: &str) -> GuestResult<()> {
    guest_call(|| crate::open_surface_title(app, showing))
}

pub fn open_font_family(family: Option<&str>) -> GuestResult<()> {
    guest_call(|| crate::open_surface_font_family(family.map(crate::FontFamily::from)))
}

pub fn navigate_back() -> GuestResult<bool> {
    guest_call(crate::navigate_back)
}

pub fn drain_tasks() -> GuestResult<()> {
    guest_call(crate::drain_tasks)
}

pub fn fire_timers() -> GuestResult<()> {
    guest_call(crate::fire_timers)
}

pub fn until_next_timer() -> GuestResult<Option<Duration>> {
    guest_call(crate::until_next_timer)
}

pub fn install_task_waker(waker: RedrawWaker) -> GuestResult<()> {
    guest_call(move || crate::set_task_waker(move || waker.wake()))
}

pub fn install_services(services: HostServices) -> GuestResult<()> {
    guest_call(move || services.install())
}
