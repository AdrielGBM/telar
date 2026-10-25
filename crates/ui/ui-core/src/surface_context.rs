//! `Surface` — one RSX surface's complete per-surface world.
//!
//! A surface (a window, or a Wayland layer-surface) owns a set of thread-local worlds: its layout tree, its size, overlay registry, dismiss stack, overlay names, focus state, input region, force-tick, title, default font family, window-command queue, and the writing direction, locale, control size, theme mode and high contrast it overrides the thread's with. Under M3 several surfaces share one UI thread and one reactive runtime, so those worlds are swappable: the runner activates a surface with [`Surface::with_entered`] around its build/event/frame, and the reactive flush re-enters the surface that owns each effect through the hook this module installs into reactive-core.
//!
//! Single-window apps never build a `Surface`: the reactive current-surface stays [`SurfaceHandle::NONE`], every effect captures `NONE`, and entering it is a no-op — so they run against the ambient thread-local worlds exactly as before, at zero added cost.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use i18n_core::LocaleContext;
use layout_reactive::{
    DirectionContext, LayoutContext, ParentsContext, ScrollHostsContext, SurfaceSizeContext,
};
use platform_core::WindowCommandContext;
use preferences_core::HighContrastContext;
use reactive_core::{
    SurfaceHandle, cancel_tasks_for, cancel_timers_for, current_surface, dispose_surface,
    in_surface_world, set_surface_enter_hook, with_current_surface,
};
use theme_core::{ControlSizeContext, ModeContext};
use ui_tree::OverlayContext;

use crate::anchor_line::PlacesContext;
use crate::annotation::AnnotationsContext;
use crate::cursor::CursorContext;
use crate::dismiss::DismissContext;
use crate::focus::FocusContext;
use crate::inherit::CascadeContext;
use crate::input_region::InputRegionContext;
use crate::named_overlay::NamedOverlaysContext;
use crate::presence::ExitsContext;
use crate::scroll_viewports::ViewportsContext;
use crate::surface_font::SurfaceFontContext;
use crate::surface_title::SurfaceTitleContext;

macro_rules! surface_worlds {
    ($( $(#[$field_meta:meta])* $field:ident: $ty:ty, )*) => {
        /// The complete per-surface world plus its reactive [`SurfaceHandle`]. Build one per window/layer-surface with [`Surface::new`]; activate it with [`Surface::with_entered`].
        pub struct Surface {
            handle: SurfaceHandle,
            $( $(#[$field_meta])* $field: $ty, )*
        }

        impl Surface {
            fn fresh_worlds(handle: SurfaceHandle) -> Self {
                Self {
                    handle,
                    $( $field: <$ty>::new_owned(), )*
                }
            }

            // Non-generic, so the chain of entries is compiled once rather than once per closure handed to `with_entered`.
            fn enter_worlds(&self, run: &mut dyn FnMut()) {
                $( let run: &mut dyn FnMut() = &mut || self.$field.with_entered(&mut *run); )*
                run()
            }

            fn enter_ambient_worlds(run: &mut dyn FnMut()) {
                $( let run: &mut dyn FnMut() = &mut || <$ty>::with_ambient_entered(&mut *run); )*
                run()
            }
        }
    };
}

surface_worlds! {
    layout: LayoutContext,
    /// Which node hangs from which, swapped with `layout` and never apart from it. Each surface's layout tree mints its node ids from its own counter, so the same `NodeId` names a different node in every surface — one shared map would have them overwrite each other's links, and a climb would leave the surface it started in. It is a separate world only because a measure closure runs inside the layout runtime's borrow and reaching back into it would re-enter.
    parents: ParentsContext,
    scroll_hosts: ScrollHostsContext,
    size: SurfaceSizeContext,
    overlay: OverlayContext,
    dismiss: DismissContext,
    named_overlays: NamedOverlaysContext,
    focus: FocusContext,
    input_region: InputRegionContext,
    exits: ExitsContext,
    cascade: CascadeContext,
    cursor: CursorContext,
    annotations: AnnotationsContext,
    viewports: ViewportsContext,
    places: PlacesContext,
    title: SurfaceTitleContext,
    font: SurfaceFontContext,
    window_commands: WindowCommandContext,
    direction: DirectionContext,
    locale: LocaleContext,
    control_size: ControlSizeContext,
    mode: ModeContext,
    high_contrast: HighContrastContext,
}

impl Surface {
    /// Allocates a fresh, inactive surface world with a unique handle and registers it so the reactive flush can re-enter it for its effects. The returned `Rc` is the sole owner; the registry keeps only a `Weak`, so dropping the `Rc` tears the surface down (and unregisters it).
    pub fn new() -> Rc<Self> {
        install_enter_hook();
        let handle = next_handle();
        // Built in the surface's own world rather than detached, so what the worlds hold is freed when the surface drops instead of never.
        let world =
            with_current_surface(handle, || in_surface_world(|| Self::fresh_worlds(handle)));
        let surface = Rc::new(world);
        SURFACES.with(|s| s.borrow_mut().insert(handle, Rc::downgrade(&surface)));
        surface
    }

    /// This surface's reactive handle. Effects registered while it is active capture it and re-run under it.
    pub fn handle(&self) -> SurfaceHandle {
        self.handle
    }

    /// The surface `handle` names on this thread, while it is alive.
    pub fn find(handle: SurfaceHandle) -> Option<Rc<Surface>> {
        SURFACES.with(|s| s.borrow().get(&handle).and_then(Weak::upgrade))
    }

    /// Runs `f` with this surface's world active, and restores the previously-active world once `f` returns or unwinds. The swapped worlds are independent thread-locals, so restore order among them is irrelevant; entries nest.
    pub fn with_entered<R>(&self, f: impl FnOnce() -> R) -> R {
        let mut f = Some(f);
        let mut out = None;
        self.enter(&mut || out = f.take().map(|f| f()));
        out.expect("entering a surface runs the closure it was handed")
    }

    fn enter(&self, run: &mut dyn FnMut()) {
        with_current_surface(self.handle, || self.enter_worlds(run));
    }

    /// Runs `run` with the ambient world active — the one that exists before any [`Surface`] is built.
    ///
    /// A single-window app never builds a surface, so its whole tree is owned by [`SurfaceHandle::NONE`] and its effects have to re-enter *this*. Without it they run against whichever surface happened to be entered when the signal fired, which is a live case as soon as one app has both — a window tree that never built a surface and a [`TextureUi`] that did.
    ///
    /// [`TextureUi`]: https://docs.rs/telar/latest/telar/struct.TextureUi.html
    fn enter_ambient(run: &mut dyn FnMut()) {
        with_current_surface(SurfaceHandle::NONE, || Self::enter_ambient_worlds(run));
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // Entered while disposing: an owner's teardown reaches into the surface-local worlds about to be dropped, and a withdrawal against whichever surface happened to be active would land on another's layout tree.
        self.with_entered(|| {
            dispose_surface(self.handle);
            // After disposal, so work a cleanup started while the surface came down is cancelled with the rest.
            cancel_tasks_for(self.handle);
            cancel_timers_for(self.handle);
        });
        SURFACES.with(|s| {
            s.borrow_mut().remove(&self.handle);
        });
    }
}

/// The surface something was registered on, for withdrawing it there whichever surface is active when it goes.
///
/// A per-surface registry is reached through the active world, and a widget is dropped wherever its last handle lets go: a tree torn down on another surface's turn would otherwise withdraw its focus stops, input regions and overlays from that surface's world, and leave its own behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HomeSurface(SurfaceHandle);

impl HomeSurface {
    pub(crate) fn here() -> Self {
        Self(current_surface())
    }

    /// Runs `f` with the surface entered, or returns `None` without running it once the surface is torn down. See [`SurfaceHandle::with_entered_if_alive`].
    pub(crate) fn with_entered<R>(self, f: impl FnOnce() -> R) -> Option<R> {
        self.0.with_entered_if_alive(f)
    }

    pub(crate) fn handle(self) -> SurfaceHandle {
        self.0
    }
}

reactive_core::dropless_thread_local! {
    // Weak, not Rc: an Rc would keep every surface alive forever and its unregistering `Drop` would never run.
    static SURFACES: released HashMap<SurfaceHandle, Weak<Surface>> = HashMap::new();
    // Handle 0 is `SurfaceHandle::NONE`, the ambient world, so real surfaces start at 1.
    static NEXT_HANDLE: Cell<u64> = const { Cell::new(1) };
    static HOOK_INSTALLED: Cell<bool> = const { Cell::new(false) };
}

fn next_handle() -> SurfaceHandle {
    NEXT_HANDLE.with(|c| {
        let id = c.get();
        c.set(id + 1);
        SurfaceHandle::from_raw(id)
    })
}

/// Installs (once per thread) the reactive-core enter-hook: given the handle an effect captured, look up its surface and run the effect with its full world active, holding the surface alive until the effect returns. Runs the effect where it is when the surface is gone (e.g. torn down while a stale effect was still scheduled).
fn install_enter_hook() {
    HOOK_INSTALLED.with(|installed| {
        if installed.replace(true) {
            return;
        }
        set_surface_enter_hook(|handle, run| {
            if handle.is_none() {
                return Surface::enter_ambient(run);
            }
            match Surface::find(handle) {
                Some(surface) => surface.enter(run),
                // Torn down while a stale effect was still scheduled: there is no world to enter, and guessing one would run it against a stranger's.
                None => run(),
            }
        });
    });
}

#[cfg(test)]
#[path = "surface_context_test.rs"]
mod tests;
