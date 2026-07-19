use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// Identifies which surface (window / layer-surface) a reactive effect belongs to. It is a cheap `Copy`
/// id: the reactive runtime stamps one onto every effect at registration and re-enters that surface's
/// context before running the effect during a flush, so an effect owned by surface A resolves its
/// layout/overlay/focus world against A even when a signal set from surface B triggers it (the "owner
/// scope" model of Solid/Floem).
///
/// [`SurfaceHandle::NONE`] is the ambient handle used when no surface context is active — single-window
/// apps, or reactive work outside any surface. Entering `NONE`, or entering the already-active surface, is
/// a no-op, so single-surface apps pay ~zero overhead.
///
/// reactive-core is the lowest crate and cannot know the per-surface thread-locals (layout/overlay/focus
/// live in higher crates), so it only stores the id and calls an installed hook ([`set_surface_enter_hook`])
/// to do the actual context switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceHandle(pub u64);

impl SurfaceHandle {
    pub const NONE: SurfaceHandle = SurfaceHandle(0);

    pub fn is_none(self) -> bool {
        self.0 == 0
    }

    /// Activates this surface's context for as long as the returned guard lives. Fast-path no-op when this
    /// surface is already active (single-window: entered once at the top, every effect re-enters the same
    /// surface → guard is inert) or when no hook is installed.
    pub fn enter(self) -> SurfaceEnterGuard {
        if self == current_surface() {
            return SurfaceEnterGuard::noop();
        }
        // Clone the hook Rc out of the borrow before calling it: the hook runs arbitrary context-swap code
        // (and may itself enter surfaces), so it must not run while this thread-local is borrowed.
        let hook = ENTER_HOOK.with(|h| h.borrow().clone());
        match hook {
            Some(f) => f(self),
            None => SurfaceEnterGuard::noop(),
        }
    }
}

impl Default for SurfaceHandle {
    fn default() -> Self {
        Self::NONE
    }
}

thread_local! {
    static CURRENT_SURFACE: Cell<SurfaceHandle> = const { Cell::new(SurfaceHandle::NONE) };
    static ENTER_HOOK: RefCell<Option<Rc<dyn Fn(SurfaceHandle) -> SurfaceEnterGuard>>> =
        const { RefCell::new(None) };
}

/// The surface an effect registered right now would be owned by.
pub fn current_surface() -> SurfaceHandle {
    CURRENT_SURFACE.with(|c| c.get())
}

/// Sets the active surface, returning the previous one (so the caller can restore it). The surface layer's
/// enter hook uses this; app code should go through `Surface::enter`.
pub fn set_current_surface(handle: SurfaceHandle) -> SurfaceHandle {
    CURRENT_SURFACE.with(|c| c.replace(handle))
}

/// Installs the thread's surface-context hook. Given a [`SurfaceHandle`], it must activate that surface's
/// full per-surface world (the reactive current-surface plus the layout/overlay/focus/... thread-locals)
/// and return a [`SurfaceEnterGuard`] that restores the previous world on drop. The higher-level `Surface`
/// layer installs this; reactive-core only knows how to call it. Without a hook, [`SurfaceHandle::enter`]
/// is a no-op — which is exactly right for single-window apps that never install one.
pub fn set_surface_enter_hook(f: impl Fn(SurfaceHandle) -> SurfaceEnterGuard + 'static) {
    ENTER_HOOK.with(|h| *h.borrow_mut() = Some(Rc::new(f)));
}

/// RAII guard restoring the surface context that was active before [`SurfaceHandle::enter`]. Produced by
/// the installed hook (carrying a restore closure) or as an inert no-op.
#[must_use = "the surface context is only active while this guard is alive"]
pub struct SurfaceEnterGuard {
    restore: Option<Box<dyn FnOnce()>>,
}

impl SurfaceEnterGuard {
    pub fn noop() -> Self {
        Self { restore: None }
    }

    pub fn new(restore: impl FnOnce() + 'static) -> Self {
        Self {
            restore: Some(Box::new(restore)),
        }
    }
}

impl Drop for SurfaceEnterGuard {
    fn drop(&mut self) {
        if let Some(f) = self.restore.take() {
            f();
        }
    }
}
