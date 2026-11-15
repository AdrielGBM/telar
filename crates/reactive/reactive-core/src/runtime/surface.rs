//! Which surface is active, and the hook that re-enters an effect's own surface before it runs.

use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;

/// Identifies which surface (window / layer-surface) a reactive effect belongs to. It is a cheap `Copy` id: the reactive runtime stamps one onto every effect at registration and re-enters that surface's context before running the effect during a flush, so an effect owned by surface A resolves its layout/overlay/focus world against A even when a signal set from surface B triggers it.
///
/// Work outside any surface — a single-window app, or reactive work no surface owns — belongs to no handle: it is `None` wherever a surface is optional, and runs in the thread's ambient world. Entering that, or entering the already-active surface, is a no-op, so single-surface apps pay ~zero overhead.
///
/// reactive-core is the lowest crate and cannot know the per-surface thread-locals (layout/overlay/focus live in higher crates), so it only stores the id and calls an installed hook ([`set_surface_enter_hook`]) to do the actual context switch.
/// A handle stays on the thread that made it: ids are minted per thread, so the same id names another surface, or none, on any other.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceHandle {
    id: NonZeroU64,
    _ui_thread_only: PhantomData<*const ()>,
}

impl SurfaceHandle {
    /// The handle a raw id names on this thread, for the surface layer minting handles and for reading one back out of the surface key a draw command carries.
    #[doc(hidden)]
    pub const fn from_raw(id: NonZeroU64) -> Self {
        Self {
            id,
            _ui_thread_only: PhantomData,
        }
    }

    /// The raw id, as the surface key a draw command carries holds it. It means something only on this thread.
    pub const fn raw(self) -> NonZeroU64 {
        self.id
    }
}

impl fmt::Debug for SurfaceHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SurfaceHandle").field(&self.id).finish()
    }
}

/// Runs `f` with `surface`'s context active, the thread's ambient world for `None`, restoring the one before once `f` returns or unwinds. Runs `f` directly when that is already active (single-window: entered once at the top, so every effect re-enters the same world) or when no hook is installed.
pub fn enter_surface<R>(surface: Option<SurfaceHandle>, f: impl FnOnce() -> R) -> R {
    if surface == current_surface() {
        return f();
    }
    // Cloned out of the borrow before calling it: the hook runs arbitrary context-swap code and may itself enter surfaces, so it must not run while this thread-local is borrowed.
    let Some(hook) = ENTER_HOOK.with(|h| h.borrow().clone()) else {
        return f();
    };
    let mut f = Some(f);
    let mut out = None;
    hook(surface, &mut || out = f.take().map(|f| f()));
    out.expect("the surface enter hook returned without running the closure it was handed")
}

/// Runs `f` with `surface`'s context active, or returns `None` without running it once that surface is gone: it took its worlds with it, and the one active then is another's. The ambient world never goes. What withdrawing something from the surface it was registered on wants, whichever surface is active when it goes.
pub fn enter_surface_if_alive<R>(
    surface: Option<SurfaceHandle>,
    f: impl FnOnce() -> R,
) -> Option<R> {
    // The hook runs the closure where it is for a surface that is gone, so only an entered one is current inside it.
    enter_surface(surface, || (current_surface() == surface).then(f))
}

type EnterHook = Rc<dyn Fn(Option<SurfaceHandle>, &mut dyn FnMut())>;

crate::dropless_thread_local! {
    static CURRENT_SURFACE: Cell<Option<SurfaceHandle>> = const { Cell::new(None) };
    static ENTER_HOOK: released Option<EnterHook> = const { None };
}

/// The surface an effect registered right now would be owned by, `None` in the ambient world.
pub fn current_surface() -> Option<SurfaceHandle> {
    CURRENT_SURFACE.with(|c| c.get())
}

/// Sets the active surface, returning the previous one (so the caller can restore it).
pub(crate) fn set_current_surface(handle: Option<SurfaceHandle>) -> Option<SurfaceHandle> {
    CURRENT_SURFACE.with(|c| c.replace(handle))
}

/// Runs `f` with `handle` as the current surface, restoring the one before once `f` returns or unwinds. For the surface layer alone, around swapping in that surface's worlds: [`enter_surface`] trusts the current surface to be entered and skips entering it again, so naming one current without its worlds live runs code against another's. App code goes through `Surface::with_entered`.
#[doc(hidden)]
pub fn with_current_surface<R>(handle: Option<SurfaceHandle>, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<SurfaceHandle>);

    impl Drop for Restore {
        fn drop(&mut self) {
            set_current_surface(self.0);
        }
    }

    let _restore = Restore(set_current_surface(handle));
    f()
}

/// Installs the thread's surface-context hook. Given a surface, `None` for the ambient world, and a closure, it must run the closure exactly once with that world active (the reactive current-surface, through `with_current_surface`, plus the layout/overlay/focus/... thread-locals), keeping the surface alive and restoring the previous world when the closure returns or unwinds; for a surface that is gone it runs the closure where it is, which is how [`enter_surface_if_alive`] tells. The higher-level `Surface` layer installs this; reactive-core only knows how to call it. Without a hook, [`enter_surface`] just runs its closure — which is exactly right for single-window apps that never install one.
pub fn set_surface_enter_hook(f: impl Fn(Option<SurfaceHandle>, &mut dyn FnMut()) + 'static) {
    let replaced = ENTER_HOOK.with(|h| h.borrow_mut().replace(Rc::new(f)));
    drop(replaced);
}

#[cfg(test)]
#[path = "surface.test.rs"]
mod tests;
