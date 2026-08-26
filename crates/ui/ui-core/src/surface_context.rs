//! `Surface` — one RSX surface's complete per-surface world.
//!
//! A surface (a window, or a Wayland layer-surface) owns a set of thread-local worlds: its layout tree,
//! overlay registry, focus state, input region, force-tick, and window-command queue. Under M3 several
//! surfaces share one UI thread and one reactive runtime, so those worlds are swappable: the runner
//! activates a surface with [`Surface::enter`] around its build/event/frame, and the reactive flush
//! re-enters the surface that owns each effect through the hook this module installs into reactive-core.
//!
//! Single-window apps never build a `Surface`: the reactive current-surface stays [`SurfaceHandle::NONE`],
//! every effect captures `NONE`, and `enter` is a no-op — so they run against the ambient thread-local
//! worlds exactly as before, at zero added cost.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use layout_reactive::{LayoutContext, LayoutGuard, ParentsContext, ParentsGuard};
use platform_core::{WindowCommandContext, WindowCommandGuard};
use reactive_core::{
    SurfaceEnterGuard, SurfaceHandle, set_current_surface, set_surface_enter_hook,
};
use services_core::{ServiceContext, ServiceGuard};
use ui_tree::{ForceTickContext, ForceTickGuard, OverlayContext, OverlayGuard};

use crate::focus::{FocusContext, FocusGuard};
use crate::input_region::{InputRegionContext, InputRegionGuard};

/// The complete per-surface world plus its reactive [`SurfaceHandle`]. Build one per window/layer-surface
/// with [`Surface::new`]; activate it with [`Surface::enter`].
pub struct Surface {
    handle: SurfaceHandle,
    layout: LayoutContext,
    /// Which node hangs from which, swapped with `layout` and never apart from it. Each surface's layout tree mints its node ids from its own counter, so the same `NodeId` names a different node in every surface — one shared map would have them overwrite each other's links, and a climb would leave the surface it started in. It is a separate world only because a measure closure runs inside the layout runtime's borrow and reaching back into it would re-enter.
    parents: ParentsContext,
    overlay: OverlayContext,
    focus: FocusContext,
    input_region: InputRegionContext,
    force_tick: ForceTickContext,
    window_commands: WindowCommandContext,
    // Per-surface DI/context scope: `provide`/`inject` (services-core) resolve against this while the surface
    // is active, so an app carries per-window context (config, theme, locale) as typed values, read even from
    // effects (the flush re-enters the surface). The generic per-surface-context primitive, à la Floem/Leptos.
    services: ServiceContext,
}

impl Surface {
    /// Allocates a fresh, inactive surface world with a unique handle and registers it so the reactive
    /// flush can re-enter it for its effects. The returned `Rc` is the sole owner; the registry keeps only a
    /// `Weak`, so dropping the `Rc` tears the surface down (and unregisters it).
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
            force_tick: ForceTickContext::new(),
            window_commands: WindowCommandContext::new(),
            services: ServiceContext::new(),
        });
        SURFACES.with(|s| s.borrow_mut().insert(handle, Rc::downgrade(&surface)));
        surface
    }

    /// This surface's reactive handle. Effects registered while it is active capture it and re-run under it.
    pub fn handle(&self) -> SurfaceHandle {
        self.handle
    }

    /// Activates this surface's world until the returned guard drops, which restores the previously-active
    /// world. The swapped worlds are independent thread-locals, so restore order among them is irrelevant;
    /// nesting `enter`s is fine.
    #[must_use = "the surface is only active while this guard is alive"]
    pub fn enter(&self) -> SurfaceGuard {
        // Set the reactive current-surface first so any effect registered while active captures this handle.
        let prev_surface = set_current_surface(self.handle);
        SurfaceGuard {
            _layout: self.layout.enter(),
            _parents: self.parents.enter(),
            _overlay: self.overlay.enter(),
            _focus: self.focus.enter(),
            _input_region: self.input_region.enter(),
            _force_tick: self.force_tick.enter(),
            _window_commands: self.window_commands.enter(),
            _services: self.services.enter(),
            _prev_surface: RestoreSurface(prev_surface),
        }
    }

    /// Activates the ambient world — the one that exists before any [`Surface`] is built.
    ///
    /// A single-window app never builds a surface, so its whole tree is owned by
    /// [`SurfaceHandle::NONE`] and its effects have to re-enter *this*. Without it they run against
    /// whichever surface happened to be entered when the signal fired, which is a live case as soon as one
    /// app has both — a window tree that never built a surface and a [`TextureUi`] that did.
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
            _force_tick: ForceTickContext::enter_ambient(),
            _window_commands: WindowCommandContext::enter_ambient(),
            _services: ServiceContext::enter_ambient(),
            _prev_surface: RestoreSurface(prev_surface),
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        SURFACES.with(|s| {
            s.borrow_mut().remove(&self.handle);
        });
    }
}

/// Restores the previously-active surface world when dropped. The per-world guards each restore their own
/// (independent) thread-local; `_prev_surface` restores the reactive current-surface.
#[must_use = "the surface is only active while this guard is alive"]
pub struct SurfaceGuard {
    _layout: LayoutGuard,
    _parents: ParentsGuard,
    _overlay: OverlayGuard,
    _focus: FocusGuard,
    _input_region: InputRegionGuard,
    _force_tick: ForceTickGuard,
    _window_commands: WindowCommandGuard,
    _services: ServiceGuard,
    _prev_surface: RestoreSurface,
}

struct RestoreSurface(SurfaceHandle);

impl Drop for RestoreSurface {
    fn drop(&mut self) {
        set_current_surface(self.0);
    }
}

thread_local! {
    // Weak, not Rc: an Rc here would keep every surface alive forever and its Drop (which unregisters) would
    // never run. The hook upgrades on demand.
    static SURFACES: RefCell<HashMap<SurfaceHandle, Weak<Surface>>> =
        RefCell::new(HashMap::new());
    // Handle 0 is SurfaceHandle::NONE (the ambient/no-surface world), so real surfaces start at 1.
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

/// Installs (once per thread) the reactive-core enter-hook: given the handle an effect captured, look up its
/// surface and activate its full world for the duration of the effect. Returns a no-op when the surface is
/// gone (e.g. torn down while a stale effect was still scheduled).
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
mod tests {
    use reactive_core::{current_surface, effect, signal};

    use super::Surface;

    // While the parent links lived in a single ambient map, namesakes from different surfaces overwrote each other and a climb begun in one surface walked another's tree — which is how a shell with eleven surfaces found an "ancestor" that was a sibling elsewhere, and hung climbing to a root that was not on that path.
    #[test]
    fn one_surfaces_parent_links_never_answer_for_another() {
        use layout_reactive::{LayoutStyle, new_container, parent};

        let a = Surface::new();
        let b = Surface::new();

        let (child_in_a, host_in_a) = {
            let _guard = a.enter();
            let child = new_container(LayoutStyle::new(), &[]).unwrap();
            let host = new_container(LayoutStyle::new(), &[child]).unwrap();
            assert_eq!(parent(child), Some(host));
            (child, host)
        };

        let _guard = b.enter();
        let child_in_b = new_container(LayoutStyle::new(), &[]).unwrap();
        assert_eq!(
            child_in_b, child_in_a,
            "the two surfaces must really collide on ids, or this proves nothing"
        );
        assert_eq!(
            parent(child_in_b),
            None,
            "a's link answered for b's node — the namesake, not the node"
        );
        assert_ne!(parent(child_in_b), Some(host_in_a));
    }

    // Two surfaces on one thread keep isolated layout worlds, and an effect built under surface A re-enters A
    // when a shared signal set "from" surface B triggers it (the M3 owner-scope contract, end-to-end).
    #[test]
    fn effect_reenters_its_surface_layout_world() {
        use layout_reactive::{
            AvailableSpace, LayoutStyle, compute_layout, new_leaf, track_layout,
        };
        use std::cell::RefCell;
        use std::rc::Rc;

        let a = Surface::new();
        let b = Surface::new();
        assert_ne!(a.handle(), b.handle());
        assert!(!a.handle().is_none());

        // A shared signal (lives in the shared runtime) plus a node built inside each surface.
        let shared = signal(0i32);

        // Build an effect under A that, on change, creates a node in A's layout world and records the handle
        // it ran under.
        let ran_under: Rc<RefCell<Vec<u64>>> = Rc::new(RefCell::new(Vec::new()));
        let ran_c = Rc::clone(&ran_under);
        let read = shared.read_only();
        let a_node = {
            let _g = a.enter();
            let (node, _) = new_leaf(LayoutStyle::new().width(10.0).height(10.0)).unwrap();
            let _e = effect(move || {
                read.get();
                ran_c.borrow_mut().push(current_surface().0);
            });
            // Keep the effect alive for the whole test by leaking it into the surface's scope via Box.
            std::mem::forget(_e);
            node
        };

        ran_under.borrow_mut().clear();

        // Set the shared signal while B is active. The flush must re-enter A for A's effect.
        {
            let _g = b.enter();
            shared.set(1);
        }
        assert_eq!(
            ran_under.borrow().as_slice(),
            &[a.handle().0],
            "A's effect must run under A's surface, not B's"
        );

        // A's node is laid out in A's world; B's world does not know it.
        {
            let _g = a.enter();
            compute_layout(
                a_node,
                AvailableSpace::Definite(100.0),
                AvailableSpace::Definite(100.0),
            )
            .unwrap();
            assert_eq!(track_layout(a_node).unwrap().get().width, 10.0);
        }
        {
            let _g = b.enter();
            assert!(
                track_layout(a_node).is_none(),
                "A's node must not exist in B's layout world"
            );
        }
    }

    // An effect that belongs to no surface has a world of its own — the ambient one — and must re-enter it when it fires. Every effect of a single-window app is one of these: the runner builds no `Surface` for one. Left un-restored they ran against whichever surface happened to be active, so a window widget re-rendering during another tree's event dispatch resolved its layout in that tree's world and found nothing there.
    #[test]
    fn an_effect_owned_by_no_surface_reenters_the_ambient_world() {
        use layout_reactive::{
            AvailableSpace, LayoutStyle, compute_layout, new_leaf, track_layout,
        };
        use std::cell::RefCell;
        use std::rc::Rc;

        use super::Surface;

        // Built with no surface active, so both the node and the effect below belong to the ambient world.
        let (ambient_node, _) = new_leaf(LayoutStyle::new().width(42.0).height(10.0)).unwrap();
        compute_layout(
            ambient_node,
            AvailableSpace::Definite(100.0),
            AvailableSpace::Definite(100.0),
        )
        .unwrap();

        let other = Surface::new();
        let shared = signal(0i32);
        let read = shared.read_only();
        let seen: Rc<RefCell<Vec<Option<f32>>>> = Rc::new(RefCell::new(Vec::new()));
        let seen_c = Rc::clone(&seen);
        let watcher = effect(move || {
            read.get();
            seen_c
                .borrow_mut()
                .push(track_layout(ambient_node).map(|rect| rect.get().width));
        });

        seen.borrow_mut().clear();
        {
            let _g = other.enter();
            shared.set(1);
        }
        assert_eq!(
            seen.borrow().as_slice(),
            &[Some(42.0)],
            "an ambient effect must resolve against the ambient layout world, not the active surface's"
        );
        drop(watcher);
    }

    // Per-surface DI/context (services-core `provide`/`inject`): each surface resolves its own value, and an
    // effect built under one surface injects THAT surface's context even when fired while another is active
    // (owner-scope re-entry now swaps the service scope too). This is what lets an app carry per-window config.
    #[test]
    fn provide_inject_is_per_surface_and_survives_into_effects() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use services_core::{provide, try_inject};

        let a = Surface::new();
        let b = Surface::new();

        {
            let _g = a.enter();
            provide(String::from("A")).unwrap();
            assert_eq!(try_inject::<String>().as_deref(), Some("A"));
        }
        {
            let _g = b.enter();
            provide(String::from("B")).unwrap();
            assert_eq!(try_inject::<String>().as_deref(), Some("B"));
        }

        let shared = signal(0i32);
        let read = shared.read_only();
        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let seen_c = Rc::clone(&seen);
        let ea = {
            let _g = a.enter();
            effect(move || {
                read.get();
                seen_c
                    .borrow_mut()
                    .push(try_inject::<String>().unwrap_or_default());
            })
        };

        seen.borrow_mut().clear();
        {
            let _g = b.enter();
            shared.set(1);
        }
        assert_eq!(
            seen.borrow().as_slice(),
            &[String::from("A")],
            "A's effect must inject A's context even when fired from B"
        );
        drop(ea);
    }

    // T-3.1 / T-8.2: a global signal (theme/locale/motion are thread-local singletons — shared across
    // surfaces on the one UI thread) written once re-runs every surface's effects, each under its OWN
    // surface context. This is what makes a single dark-mode toggle update all windows correctly.
    #[test]
    fn global_signal_reruns_all_surfaces_each_under_its_context() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let a = Surface::new();
        let b = Surface::new();

        // A shared "global" signal, standing in for the theme/locale signal both surfaces read.
        let global = signal(0i32);

        // Each surface registers an effect reading the global signal; each records the surface it ran under.
        let log: Rc<RefCell<Vec<(char, u64)>>> = Rc::new(RefCell::new(Vec::new()));

        let log_a = Rc::clone(&log);
        let read_a = global.read_only();
        let ea = {
            let _g = a.enter();
            effect(move || {
                read_a.get();
                log_a.borrow_mut().push(('a', current_surface().0));
            })
        };

        let log_b = Rc::clone(&log);
        let read_b = global.read_only();
        let eb = {
            let _g = b.enter();
            effect(move || {
                read_b.get();
                log_b.borrow_mut().push(('b', current_surface().0));
            })
        };

        log.borrow_mut().clear();

        // A single global write re-runs both surfaces' effects, each under its own context.
        global.set(1);

        let entries = log.borrow().clone();
        assert!(
            entries.contains(&('a', a.handle().0)),
            "A's effect must re-run under A: {entries:?}"
        );
        assert!(
            entries.contains(&('b', b.handle().0)),
            "B's effect must re-run under B: {entries:?}"
        );

        drop(ea);
        drop(eb);
    }
}
