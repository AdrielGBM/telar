//! Handles into the thread-local runtime stay on the thread that made them, checked by compile-fail doctests.

/// The handles are still `Copy` whatever they hold, which also proves the paths the compile-fail blocks below use resolve.
///
/// ```
/// use telar_reactive_core::{Effect, Memo, OwnerId, ReadSignal, RwSignal, SurfaceHandle};
/// fn assert_copy<T: Copy>() {}
/// assert_copy::<ReadSignal<String>>();
/// assert_copy::<RwSignal<String>>();
/// assert_copy::<Memo<String>>();
/// assert_copy::<Effect>();
/// assert_copy::<OwnerId>();
/// assert_copy::<SurfaceHandle>();
/// ```
pub struct HandlesStayCopy;

/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<telar_reactive_core::SurfaceHandle>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<telar_reactive_core::SurfaceHandle>();
/// ```
pub struct SurfaceHandleIsThreadBound;

/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<telar_reactive_core::ReadSignal<i32>>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<telar_reactive_core::ReadSignal<i32>>();
/// ```
pub struct ReadSignalIsThreadBound;

/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<telar_reactive_core::RwSignal<i32>>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<telar_reactive_core::RwSignal<i32>>();
/// ```
pub struct RwSignalIsThreadBound;

/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<telar_reactive_core::Memo<i32>>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<telar_reactive_core::Memo<i32>>();
/// ```
pub struct MemoIsThreadBound;

/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<telar_reactive_core::Effect>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<telar_reactive_core::Effect>();
/// ```
pub struct EffectIsThreadBound;

/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<telar_reactive_core::OwnerId>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<telar_reactive_core::OwnerId>();
/// ```
pub struct OwnerIdIsThreadBound;

/// The bug the marker closes: a handle moved into a worker would read that thread's runtime, where its id names some other signal or none.
///
/// ```compile_fail,E0277
/// let count = telar_reactive_core::signal(0);
/// std::thread::spawn(move || count.set(1));
/// ```
///
/// ```compile_fail,E0277
/// let count = telar_reactive_core::signal(0);
/// telar_reactive_core::spawn_task(move || count.set(1), |()| {});
/// ```
pub struct WorkerCannotCaptureASignal;

/// Dropping the handle deregisters by id from the dropping thread's runtime, which on another thread is some other callback or none.
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<telar_reactive_core::FlushNotifyHandle>();
/// ```
///
/// ```compile_fail,E0277
/// fn assert_send<T: Send>() {}
/// assert_send::<telar_reactive_core::OpenBatch>();
/// ```
pub struct RuntimeGuardsAreThreadBound;
