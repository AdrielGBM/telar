//! `Surface` — one RSX surface's complete per-surface world.
//!
//! A surface (a window, or a Wayland layer-surface) owns a set of thread-local worlds: its layout tree, overlay registry, focus state, input region, force-tick, and window-command queue. Under M3 several surfaces share one UI thread and one reactive runtime, so those worlds are swappable: the runner activates a surface with [`Surface::enter`] around its build/event/frame, and the reactive flush re-enters the surface that owns each effect through the hook this module installs into reactive-core.
//!
//! Single-window apps never build a `Surface`: the reactive current-surface stays [`SurfaceHandle::NONE`], every effect captures `NONE`, and `enter` is a no-op — so they run against the ambient thread-local worlds exactly as before, at zero added cost.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use layout_reactive::{LayoutContext, LayoutGuard, ParentsContext, ParentsGuard};
use platform_core::{WindowCommandContext, WindowCommandGuard};
use reactive_core::{
    SurfaceEnterGuard, SurfaceHandle, dispose_surface_owners, set_current_surface,
    set_surface_enter_hook,
};
use ui_tree::{OverlayContext, OverlayGuard};

use crate::focus::{FocusContext, FocusGuard};
use crate::input_region::{InputRegionContext, InputRegionGuard};

/// The complete per-surface world plus its reactive [`SurfaceHandle`]. Build one per window/layer-surface with [`Surface::new`]; activate it with [`Surface::enter`].
pub struct Surface {
    handle: SurfaceHandle,
    layout: LayoutContext,
    /// Which node hangs from which, swapped with `layout` and never apart from it. Each surface's layout tree mints its node ids from its own counter, so the same `NodeId` names a different node in every surface — one shared map would have them overwrite each other's links, and a climb would leave the surface it started in. It is a separate world only because a measure closure runs inside the layout runtime's borrow and reaching back into it would re-enter.
    parents: ParentsContext,
    overlay: OverlayContext,
    focus: FocusContext,
    input_region: InputRegionContext,
    window_commands: WindowCommandContext,
}

impl Surface {
    /// Allocates a fresh, inactive surface world with a unique handle and registers it so the reactive flush can re-enter it for its effects. The returned `Rc` is the sole owner; the registry keeps only a `Weak`, so dropping the `Rc` tears the surface down (and unregisters it).
    pub fn new() -> Rc<Self> {
        install_enter_hook();
        let handle = next_handle();
        let surface = Rc::new(Self {
            handle,
            layout: LayoutContext::new(),
            parents: ParentsContext::new(),
            overlay: OverlayContext::new(),
            focus: FocusContext::new(),
            input_region: InputRegionContext::new(),
            window_commands: WindowCommandContext::new(),
        });
        SURFACES.with(|s| s.borrow_mut().insert(handle, Rc::downgrade(&surface)));
        surface
    }

    /// This surface's reactive handle. Effects registered while it is active capture it and re-run under it.
    pub fn handle(&self) -> SurfaceHandle {
        self.handle
    }

    /// Activates this surface's world until the returned guard drops, which restores the previously-active world. The swapped worlds are independent thread-locals, so restore order among them is irrelevant; nesting `enter`s is fine.
    #[must_use = "the surface is only active while this guard is alive"]
    pub fn enter(&self) -> SurfaceGuard {
        // Set first, so any effect registered while active captures this handle.
        let prev_surface = set_current_surface(self.handle);
        SurfaceGuard {
            _layout: self.layout.enter(),
            _parents: self.parents.enter(),
            _overlay: self.overlay.enter(),
            _focus: self.focus.enter(),
            _input_region: self.input_region.enter(),
            _window_commands: self.window_commands.enter(),
            _prev_surface: RestoreSurface(prev_surface),
        }
    }

    /// Activates the ambient world — the one that exists before any [`Surface`] is built.
    ///
    /// A single-window app never builds a surface, so its whole tree is owned by [`SurfaceHandle::NONE`] and its effects have to re-enter *this*. Without it they run against whichever surface happened to be entered when the signal fired, which is a live case as soon as one app has both — a window tree that never built a surface and a [`TextureUi`] that did.
    ///
    /// [`TextureUi`]: https://docs.rs/telar/latest/telar/struct.TextureUi.html
    #[must_use = "the ambient world is only active while this guard is alive"]
    fn enter_ambient() -> SurfaceGuard {
        let prev_surface = set_current_surface(SurfaceHandle::NONE);
        SurfaceGuard {
            _layout: LayoutContext::enter_ambient(),
            _parents: ParentsContext::enter_ambient(),
            _overlay: OverlayContext::enter_ambient(),
            _focus: FocusContext::enter_ambient(),
            _input_region: InputRegionContext::enter_ambient(),
            _window_commands: WindowCommandContext::enter_ambient(),
            _prev_surface: RestoreSurface(prev_surface),
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // Entered while disposing: an owner's teardown reaches into the surface-local worlds about to be dropped, and a withdrawal against whichever surface happened to be active would land on another's layout tree.
        {
            let _entered = self.enter();
            dispose_surface_owners(self.handle);
        }
        SURFACES.with(|s| {
            s.borrow_mut().remove(&self.handle);
        });
    }
}

/// Restores the previously-active surface world when dropped. The per-world guards each restore their own (independent) thread-local; `_prev_surface` restores the reactive current-surface.
#[must_use = "the surface is only active while this guard is alive"]
pub struct SurfaceGuard {
    _layout: LayoutGuard,
    _parents: ParentsGuard,
    _overlay: OverlayGuard,
    _focus: FocusGuard,
    _input_region: InputRegionGuard,
    _window_commands: WindowCommandGuard,
    _prev_surface: RestoreSurface,
}

struct RestoreSurface(SurfaceHandle);

impl Drop for RestoreSurface {
    fn drop(&mut self) {
        set_current_surface(self.0);
    }
}

thread_local! {
    // Weak, not Rc: an Rc would keep every surface alive forever and its unregistering `Drop` would never run.
    static SURFACES: RefCell<HashMap<SurfaceHandle, Weak<Surface>>> =
        RefCell::new(HashMap::new());
    // Handle 0 is `SurfaceHandle::NONE`, the ambient world, so real surfaces start at 1.
    static NEXT_HANDLE: Cell<u64> = const { Cell::new(1) };
    static HOOK_INSTALLED: Cell<bool> = const { Cell::new(false) };
}

fn next_handle() -> SurfaceHandle {
    NEXT_HANDLE.with(|c| {
        let id = c.get();
        c.set(id + 1);
        SurfaceHandle(id)
    })
}

/// Installs (once per thread) the reactive-core enter-hook: given the handle an effect captured, look up its surface and activate its full world for the duration of the effect. Returns a no-op when the surface is gone (e.g. torn down while a stale effect was still scheduled).
fn install_enter_hook() {
    HOOK_INSTALLED.with(|installed| {
        if installed.replace(true) {
            return;
        }
        set_surface_enter_hook(|handle| {
            if handle.is_none() {
                let guard = Surface::enter_ambient();
                return SurfaceEnterGuard::new(move || drop(guard));
            }
            let surface = SURFACES.with(|s| s.borrow().get(&handle).and_then(Weak::upgrade));
            match surface {
                Some(surface) => {
                    let guard = surface.enter();
                    SurfaceEnterGuard::new(move || drop(guard))
                }
                // Torn down while a stale effect was still scheduled: there is no world to enter, and guessing one would run it against a stranger's.
                None => SurfaceEnterGuard::noop(),
            }
        });
    });
}

#[cfg(test)]
#[path = "surface_context_test.rs"]
mod tests;
