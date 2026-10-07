//! Embedded apps: hosting a separately-`dlopen`'d rsx UI inside a host window.
//!
//! Unlike hot-reload (dev-only, one dylib that *replaces* the whole window), an embedded app is a production capability: the host stays a full rsx app and embeds one or more guests, each a cdylib with its **own** reactive/layout/overlay/motion runtime (separate thread-locals, because each dylib statically links its own copy of the runtime crates). The host cannot reach into that runtime, so — exactly as hot-reload does for its single dylib — it *drives* the guest across the FFI boundary through exported shims.
//!
//! The novel part hot-reload never needed is **compositing two runtimes into one window**: the guest flattens its own view tree to a self-contained `Vec<DrawCommand>` ([`EmbedInstance::paint`]) and hands it back; the host translates + clips those commands into the guest's sub-rect and splices them into its own frame. No offscreen texture, no shared GPU device — the host's renderer paints everything in one pass.
//!
//! Layering: this crate is app-agnostic. A guest author implements [`EmbeddedApp`] (or an adapter to it) and calls the [`embed!`](crate::embed) macro to export the shims. The host enables `host`, calls [`load_embedded`] and drives the returned [`LoadedEmbed`]. Nothing here knows about any particular app.
//!
//! **A second dependency, like `telar-dynamic`.** It knows nothing of `telar` — a guest builds a `ui-core` tree and hands back draw commands, which is a layer below the facade — so it is the application that names both, and keeping them on one version is the same lockstep `telar` and `telar-macros` already have.
//!
//! # Feature flags
#![cfg_attr(
    feature = "document-features",
    doc = document_features::document_features!()
)]
#![warn(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

use std::cell::RefCell;
use std::rc::Rc;
use web_time::Instant;

use geometry_core::Rect;
use layout_core::AvailableSpace;
use platform_core::{Event, SystemPreferences, WindowCommand};
use renderer_core::{BorderRadius, Color, DrawCommand};
use ui_core::{ComponentList, EventResult, NodeId, Surface, compute_layout, mark_dirty};
use ui_tree::{Component, RenderNode};

use platform_core::AppCtx;

/// Owned draw-command list returned across the FFI boundary (the guest's flattened frame). Self-contained: baked geometry, `Arc`-shared styles/data — the host can render it directly (same-toolchain ABI, as with hot-reload's `Vec<WindowCommand>`).
pub type DrawList = Vec<DrawCommand>;
/// Owned window-command list drained from the guest's own queue (a title bar's drag/close etc.).
pub type WindowCommands = Vec<WindowCommand>;

// Re-exported for the `embed!` macro's shim signatures, so a guest crate needs only `telar_embed::*`.
pub use platform_core::Event as EmbedEvent;
pub use renderer_core::Color as EmbedColor;

/// Wrap a guest's painted [`DrawList`] into a [`RenderNode`] the host splices into its own frame: translated to `rect`'s origin (the guest paints in its own `(0,0)` space) and clipped to `rect` (so it can't draw over the host chrome). The exact idiom rsx's own scroll area uses to host a sub-tree in a viewport.
///
/// `image_salt` namespaces the guest's image ids into a distinct range. Each dylib allocates `ImageData` ids from its own process-local counter starting at 1, so a guest's ids would otherwise alias the host's (and other guests') in the shared renderer's texture cache; a distinct nonzero salt per guest instance keeps them apart. Pass `0` to skip (single-runtime callers).
pub fn composite(rect: Rect, image_salt: u64, mut commands: DrawList) -> RenderNode {
    if image_salt != 0 {
        // Original ids are small and monotonic, so shifting the salt above them keeps `(salt, id)` unique and stable frame-to-frame and the texture cache still hits.
        const ID_BITS: u32 = 40;
        for cmd in &mut commands {
            if let DrawCommand::Image { data, .. } = cmd {
                let salted = (image_salt << ID_BITS) | (data.id & ((1u64 << ID_BITS) - 1));
                std::sync::Arc::make_mut(data).id = salted;
            }
        }
    }
    RenderNode::clip(
        rect,
        BorderRadius::zero(),
        [RenderNode::translate(
            rect.x,
            rect.y,
            commands.into_iter().map(RenderNode::Primitive),
        )],
    )
}

/// An embeddable rsx UI a host can drive as a guest. The generic union of "build a view tree, render it, handle events, run per-frame background work, and present a title/icon" — no app-specific semantics. A concrete app (or an adapter over one) implements this; the [`embed!`](crate::embed) macro exports it.
///
/// Lifecycle the driver enforces: [`build`](Self::build) runs once, inside the guest's freshly-entered [`Surface`], so the content's layout nodes land in *this* surface's world; afterwards [`layout_root`](Self::layout_root) is the node the driver sizes to the host's sub-rect.
pub trait EmbeddedApp: 'static {
    /// Build the content's layout tree. Called once by the driver with the guest's surface active, so nodes are allocated in this surface's layout world. [`layout_root`](Self::layout_root) must be valid after it.
    ///
    /// **Also where a guest installs what only a runner would have installed for it.** A guest is a `cdylib`: it links its own copy of every crate behind the facade, so the statics and thread-locals those keep are its own and start empty. The theme is one; `TextMetrics` is the one that bites, because nothing sets a default and the first text laid out here panics without it — call `telar::install_default_text_metrics()` (or `renderer_core::set_text_metrics` with whatever suits the surface) before building the tree. The driver cannot do it: it does not know whether the guest draws to pixels or to cells, and a trait object cannot cross the boundary to be handed over.
    fn build(&mut self);

    /// The content's top layout node — the one the driver `compute_layout`s to the host-assigned rect size.
    fn layout_root(&self) -> NodeId;

    /// Render the content to a [`RenderNode`] (flattened by the driver into the returned [`DrawList`]).
    fn view(&self) -> RenderNode;

    /// Route an event (already translated into the content's local coordinate space by the host).
    fn on_event(&mut self, event: &Event) -> EventResult;

    /// Re-lay-out internal scroll viewports after the driver has laid out the root at the new size. No-op for content without its own scroll roots.
    fn relayout_viewports(&mut self) {}

    /// Called when the content becomes visible (the host activated its tab). Autofocus the primary input here.
    fn activate(&mut self) {}

    /// Drain background-work channels into signals (see [`platform_core::RedrawWaker`]); the host forwards its `ctx`.
    fn on_frame(&mut self, _ctx: &mut AppCtx) {}

    /// The window/tab clear color, if the content wants one.
    fn clear_color(&self) -> Option<Color> {
        None
    }

    /// The guest's display title (may read signals — the driver reads it with the surface active).
    fn title(&self) -> String;

    /// The guest's icon bytes, owned so they cross the FFI boundary safely (no borrow into the dylib image).
    fn icon(&self) -> Option<Vec<u8>> {
        None
    }

    /// A stable identifier for this app kind (routing, discovery).
    fn id(&self) -> String;
}

/// Bridges the driver's [`ComponentList`] (which owns its root [`Component`]) to the shared [`EmbeddedApp`], so the driver keeps its own handle to call `activate`/`relayout_viewports`/metadata while the segment tree renders and dispatches events through the same object. Single-threaded; the borrows never overlap (paint borrows during `commands()`, events during `on_event`, driver calls in between).
struct EmbeddedComponent(Rc<RefCell<Box<dyn EmbeddedApp>>>);

impl Component for EmbeddedComponent {
    fn view(&self) -> RenderNode {
        self.0.borrow().view()
    }
    fn on_event(&mut self, event: &Event) -> EventResult {
        self.0.borrow_mut().on_event(event)
    }
    fn debug_name(&self) -> &'static str {
        "EmbedRoot"
    }
}

/// The dylib-side guest driver: a headless single-surface runtime (no window, no renderer) that the host drives across the FFI boundary. Owns the guest's [`Surface`] and its [`ComponentList`]; every method enters the surface first, so all work touches this guest's thread-local worlds, not the host's.
///
/// The host holds this only as an opaque `*mut EmbedInstance` (it never dereferences it — every call goes through an exported shim so the code runs in the dylib). Constructed by [`__embed_create`].
pub struct EmbedInstance {
    embedded: Rc<RefCell<Box<dyn EmbeddedApp>>>,
    tree: ComponentList,
    root: NodeId,
    size: (f32, f32),
    task_waker_installed: bool,
    // Declared last so it drops last: the content and segment tree free their state while this surface's worlds still exist.
    surface: Rc<Surface>,
}

impl EmbedInstance {
    /// Build the guest: allocate its surface, build the content tree inside it, and mount the segment tree.
    pub fn new(embedded: Box<dyn EmbeddedApp>) -> Self {
        let surface = Surface::new();
        let embedded = Rc::new(RefCell::new(embedded));
        let (root, tree) = {
            let _g = surface.enter();
            embedded.borrow_mut().build();
            let root = embedded.borrow().layout_root();
            let tree = ComponentList::new(EmbeddedComponent(Rc::clone(&embedded)));
            (root, tree)
        };
        Self {
            surface,
            embedded,
            tree,
            root,
            size: (0.0, 0.0),
            task_waker_installed: false,
        }
    }

    /// Lay the content out to the host-assigned sub-rect size, which is this guest's surface size, then let it re-lay-out its own scroll viewports.
    pub fn relayout(&mut self, width: f32, height: f32) {
        let _g = self.surface.enter();
        self.size = (width, height);
        ui_core::set_surface_size(geometry_core::Size::new(width, height));
        let _ = mark_dirty(self.root);
        let _ = compute_layout(
            self.root,
            AvailableSpace::Definite(width),
            AvailableSpace::Definite(height),
        );
        // So a signal the content writes flushes after the `borrow_mut` is released; a synchronous flush would re-run the segment's `view()`, which borrows the same `RefCell`.
        let embedded = &self.embedded;
        reactive_core::batch(|| embedded.borrow_mut().relayout_viewports());
    }

    /// Re-lay-out only what the guest's own reactive changes dirtied (a list grew, a panel toggled), at the last size given to [`relayout`](Self::relayout). Driven every frame by the host — the analog of the runner calling `App::relayout` (`ui_core::relayout_if_dirty`) on an in-process app.
    pub fn relayout_dirty(&self) {
        let _g = self.surface.enter();
        ui_core::relayout_if_dirty();
    }

    /// The guest's current frame as a flat, self-contained command list. The host translates it into the guest's sub-rect and splices it into its own frame.
    pub fn paint(&self) -> DrawList {
        let _g = self.surface.enter();
        self.tree.commands().clone()
    }

    /// The content generation; unchanged between two reads means [`paint`](Self::paint) would return the same commands, so the host can skip re-fetching (mirrors the host renderer's idle-blit gate).
    pub fn generation(&self) -> u64 {
        let _g = self.surface.enter();
        self.tree.generation()
    }

    /// Whether an animation is still in flight in this guest's motion engine.
    pub fn motion_active(&self) -> bool {
        let _g = self.surface.enter();
        motion_core::has_active()
    }

    /// Dispatch an event to the content (already in local coordinates). Self-batches in the guest's runtime.
    pub fn on_event(&mut self, event: &Event) -> bool {
        let _g = self.surface.enter();
        // A cdylib carries its own copy of every `thread_local` in ui-core, so observing on the host's side left the guest's widgets reading a permanently empty registry. `HotTree::on_event` carries these for the same reason.
        ui_core::observe_keyboard(event);
        ui_core::observe_pointer(event);
        self.tree.on_event(event) == EventResult::Handled
    }

    /// Route a positioned event to the guest's overlay layer (modals/dropdowns) with priority; `true` means an overlay consumed it and the host should not fall through to the content.
    ///
    /// The host calls this before [`on_event`](Self::on_event) and stops when it returns `true`, so the registries are fed here too — otherwise an event an overlay consumes never reaches them at all.
    pub fn dispatch_overlays(&self, event: &Event) -> bool {
        let _g = self.surface.enter();
        ui_core::observe_keyboard(event);
        ui_core::observe_pointer(event);
        // So an overlay handler's signal writes flush after dispatch rather than mid-walk.
        reactive_core::batch(|| ui_core::dispatch_overlays(event) == EventResult::Handled)
    }

    /// Closes the frame on this side of the boundary, for the same reason [`on_event`](Self::on_event) observes on it: `key_pressed` answers for one frame, and the frame it answers for is the one whose widgets asked.
    pub fn end_frame(&self) {
        let _g = self.surface.enter();
        ui_core::end_keyboard_frame();
    }

    /// Advance the guest's motion engine and flush its runtime so animations progress and re-render.
    pub fn motion_tick(&self, now: Instant) {
        let _g = self.surface.enter();
        reactive_core::begin_batch();
        motion_core::tick(now);
        reactive_core::end_batch();
    }

    /// Drain window-management commands the guest's UI enqueued (its title bar drag/minimize/close).
    pub fn drain_window_commands(&self) -> WindowCommands {
        let _g = self.surface.enter();
        platform_core::take_window_commands()
    }

    /// Write the user's system preferences into the guest's own copy of the store, which its views, its theme's `follow_system` and its motion engine read.
    pub fn set_system_preferences(&self, preferences: &SystemPreferences) {
        let _g = self.surface.enter();
        preferences_core::set_system_preferences(preferences.clone());
    }

    /// Run the guest's per-frame background-work hook, forwarding the host's `ctx` (so a guest worker thread can wake the host loop via `ctx.redraw_waker()`, just as an in-process app does).
    pub fn on_frame(&mut self, ctx: &mut AppCtx) {
        let _g = self.surface.enter();
        // The guest links its own reactive-core copy, so `spawn_task` inside it registers in a runtime the host cannot reach. Both halves of the bridge are wired here rather than through new FFI symbols.
        if !self.task_waker_installed
            && let Some(waker) = ctx.redraw_waker()
        {
            reactive_core::set_task_waker(move || waker.wake());
            self.task_waker_installed = true;
        }
        reactive_core::drain_tasks();
        // So signals the hook writes flush after the `borrow_mut` releases, never re-entering `view()` mid-borrow.
        let embedded = &self.embedded;
        reactive_core::batch(|| embedded.borrow_mut().on_frame(ctx));
    }

    /// Autofocus/announce the content becoming visible; re-render so a focus change shows this frame.
    pub fn activate(&mut self) {
        let _g = self.surface.enter();
        let embedded = &self.embedded;
        reactive_core::batch(|| embedded.borrow_mut().activate());
    }

    pub fn clear_color(&self) -> Option<Color> {
        let _g = self.surface.enter();
        self.embedded.borrow().clear_color()
    }

    pub fn title(&self) -> String {
        let _g = self.surface.enter();
        self.embedded.borrow().title()
    }

    pub fn icon(&self) -> Option<Vec<u8>> {
        let _g = self.surface.enter();
        self.embedded.borrow().icon()
    }

    pub fn id(&self) -> String {
        let _g = self.surface.enter();
        self.embedded.borrow().id()
    }
}

// The `embed!` macro exports one thin `#[no_mangle]` wrapper per method, each forwarding to one of these. `#[doc(hidden)]`: public only because the expansion lands in the guest crate.

impl Drop for EmbedInstance {
    fn drop(&mut self) {
        // Callbacks close over this surface's state, so its work must not outlive it. Scoped to this instance, because two instances of one guest dylib share a task registry and a blanket reset would cancel both.
        reactive_core::cancel_tasks_for(self.surface.handle());
    }
}

/// Build a guest instance and leak it to a raw pointer the host owns (freed via [`__embed_destroy`]).
#[doc(hidden)]
pub fn __embed_create(embedded: Box<dyn EmbeddedApp>) -> *mut EmbedInstance {
    Box::into_raw(Box::new(EmbedInstance::new(embedded)))
}

/// # Safety `inst` must be a pointer returned by [`__embed_create`] and not yet destroyed.
#[doc(hidden)]
pub unsafe fn __embed_destroy(inst: *mut EmbedInstance) {
    drop(unsafe { Box::from_raw(inst) });
}

macro_rules! embed_shim {
    ($(#[$m:meta])* $vis_fn:ident ($($arg:ident : $ty:ty),*) $(-> $ret:ty)? => $method:ident) => {
        $(#[$m])*
        #[doc(hidden)]
        /// # Safety `inst` must be a live pointer from [`__embed_create`].
        pub unsafe fn $vis_fn(inst: *mut EmbedInstance $(, $arg: $ty)*) $(-> $ret)? {
            unsafe { (*inst).$method($($arg),*) }
        }
    };
}

embed_shim!(__embed_relayout(width: f32, height: f32) => relayout);
embed_shim!(__embed_relayout_dirty() => relayout_dirty);
embed_shim!(__embed_paint() -> DrawList => paint);
embed_shim!(__embed_generation() -> u64 => generation);
embed_shim!(__embed_on_event(event: &Event) -> bool => on_event);
embed_shim!(__embed_dispatch_overlays(event: &Event) -> bool => dispatch_overlays);
embed_shim!(__embed_end_frame() => end_frame);
embed_shim!(__embed_motion_tick(now: Instant) => motion_tick);
embed_shim!(__embed_motion_active() -> bool => motion_active);
embed_shim!(__embed_drain_window_commands() -> WindowCommands => drain_window_commands);
embed_shim!(__embed_set_system_preferences(preferences: &SystemPreferences) => set_system_preferences);
embed_shim!(__embed_activate() => activate);
embed_shim!(__embed_clear_color() -> Option<Color> => clear_color);
embed_shim!(__embed_title() -> String => title);
embed_shim!(__embed_icon() -> Option<Vec<u8>> => icon);
embed_shim!(__embed_id() -> String => id);

// `on_frame` takes `&mut AppCtx`, whose lifetime the shim macro cannot spell.
/// # Safety `inst` must be a live pointer from [`__embed_create`].
#[doc(hidden)]
pub unsafe fn __embed_on_frame(inst: *mut EmbedInstance, ctx: &mut AppCtx) {
    unsafe { (*inst).on_frame(ctx) }
}

/// The version of the guest/host contract below. Bump it whenever [`EmbedVTable`] changes shape — adding a field, reordering one, or changing a signature — so a stale `.so` is refused with a version mismatch instead of being called through a table whose fields have moved under it. A type that crosses the boundary inside an event or a draw command changing layout bumps it too: 5 is for `Semantics` gaining `current`, which `Element` carries across, and `Element` gaining `arrival_margin`.
pub const TELAR_EMBED_ABI: u32 = 5;

/// Everything the host calls on a guest, as one exported symbol.
///
/// `#[repr(C)]` is what makes the version check sound rather than cosmetic: `abi` is guaranteed to sit at offset 0, so the host can read it out of a guest built against a different (possibly shorter) table before it reads anything else.
///
/// The signatures use the `extern "Rust"` ABI over Rust types, so a guest must be built with the same toolchain as its host — a first-party guest model, exactly as hot reload requires.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct EmbedVTable {
    pub abi: u32,
    pub create: unsafe extern "Rust" fn(&[String]) -> *mut EmbedInstance,
    pub destroy: unsafe extern "Rust" fn(*mut EmbedInstance),
    pub relayout: unsafe extern "Rust" fn(*mut EmbedInstance, f32, f32),
    pub relayout_dirty: unsafe extern "Rust" fn(*mut EmbedInstance),
    pub paint: unsafe extern "Rust" fn(*mut EmbedInstance) -> DrawList,
    pub generation: unsafe extern "Rust" fn(*mut EmbedInstance) -> u64,
    pub on_event: unsafe extern "Rust" fn(*mut EmbedInstance, &Event) -> bool,
    pub dispatch_overlays: unsafe extern "Rust" fn(*mut EmbedInstance, &Event) -> bool,
    pub end_frame: unsafe extern "Rust" fn(*mut EmbedInstance),
    pub motion_tick: unsafe extern "Rust" fn(*mut EmbedInstance, Instant),
    pub motion_active: unsafe extern "Rust" fn(*mut EmbedInstance) -> bool,
    pub drain_window_commands: unsafe extern "Rust" fn(*mut EmbedInstance) -> WindowCommands,
    pub set_system_preferences: unsafe extern "Rust" fn(*mut EmbedInstance, &SystemPreferences),
    pub activate: unsafe extern "Rust" fn(*mut EmbedInstance),
    pub clear_color: unsafe extern "Rust" fn(*mut EmbedInstance) -> Option<Color>,
    pub title: unsafe extern "Rust" fn(*mut EmbedInstance) -> String,
    pub icon: unsafe extern "Rust" fn(*mut EmbedInstance) -> Option<Vec<u8>>,
    pub id: unsafe extern "Rust" fn(*mut EmbedInstance) -> String,
    pub on_frame: unsafe extern "Rust" fn(*mut EmbedInstance, &mut AppCtx),
}

/// Exports a guest cdylib's one FFI symbol, the `_rsx_embed_vtable`. `$factory` is any `Fn(&[String]) -> Box<dyn EmbeddedApp>` — invoked once per instance with the launch args.
///
/// ```ignore
/// telar_embed::embed!(|args: &[String]| -> Box<dyn telar_embed::EmbeddedApp> { Box::new(MyApp::new(args)) });
/// ```
///
/// One symbol rather than one per method, so adding a guest method is a field here and a wrapper on the host instead of four edits across two macros — and so a stale `.so` fails the [`TELAR_EMBED_ABI`] check with a version mismatch rather than a missing-symbol error that names whichever method happened to be added last.
///
/// The symbol is a plain (release) export — no `hot-reload` feature, no `dev` feature.
#[macro_export]
macro_rules! embed {
    ($factory:expr) => {
        #[unsafe(no_mangle)]
        pub static _rsx_embed_vtable: $crate::EmbedVTable = {
            unsafe extern "Rust" fn create(
                args: &[::std::string::String],
            ) -> *mut $crate::EmbedInstance {
                $crate::__embed_create(($factory)(args))
            }
            $crate::EmbedVTable {
                abi: $crate::TELAR_EMBED_ABI,
                create,
                destroy: $crate::__embed_destroy,
                relayout: $crate::__embed_relayout,
                relayout_dirty: $crate::__embed_relayout_dirty,
                paint: $crate::__embed_paint,
                generation: $crate::__embed_generation,
                on_event: $crate::__embed_on_event,
                dispatch_overlays: $crate::__embed_dispatch_overlays,
                end_frame: $crate::__embed_end_frame,
                motion_tick: $crate::__embed_motion_tick,
                motion_active: $crate::__embed_motion_active,
                drain_window_commands: $crate::__embed_drain_window_commands,
                set_system_preferences: $crate::__embed_set_system_preferences,
                activate: $crate::__embed_activate,
                clear_color: $crate::__embed_clear_color,
                title: $crate::__embed_title,
                icon: $crate::__embed_icon,
                id: $crate::__embed_id,
                on_frame: $crate::__embed_on_frame,
            }
        };
    };
}

#[cfg(feature = "host")]
pub use host::{LoadedEmbed, load_embedded};

#[cfg(feature = "host")]
mod host {
    use super::*;
    use std::path::Path;

    /// A loaded guest the host drives. Holds the live instance (dylib-allocated) and the `Library` that must outlive it. `!Send`/`!Sync`: the instance is a foreign reactive runtime, driven only on the UI thread.
    pub struct LoadedEmbed {
        inst: *mut EmbedInstance,
        vtable: EmbedVTable,
        // Declared last so it drops last: the instance is destroyed before the library unmaps.
        _lib: libloading::Library,
    }

    /// Load a guest cdylib and create one instance from it (calling its vtable's `create` with `args`).
    ///
    /// The library is kept mapped for the guest's lifetime — the instance holds live pointers into the dylib's code and data.
    pub fn load_embedded(
        path: &Path,
        args: &[String],
    ) -> Result<LoadedEmbed, Box<dyn std::error::Error>> {
        let lib = platform_core::guest::open(path)?;

        let symbol: libloading::Symbol<*const EmbedVTable> =
            unsafe { lib.get(b"_rsx_embed_vtable\0")? };
        let vtable = unsafe { read_vtable(*symbol) }
            .map_err(|mismatch| format!("{mismatch} — rebuild {}", path.display()))?;

        let inst = unsafe { (vtable.create)(args) };
        if inst.is_null() {
            return Err("guest create returned null".into());
        }
        let guest = LoadedEmbed {
            inst,
            vtable,
            _lib: lib,
        };
        // A guest starts with a store of its own that knows nothing, and a host only forwards changes; without this one opened mid-session would draw light on a dark desktop until the user next changed a setting.
        guest.set_system_preferences(&preferences_core::system_preferences());
        Ok(guest)
    }

    /// Copies a guest's table out after checking it was built against this one.
    ///
    /// # Safety
    /// `ptr` must point at a guest's exported table, of this or any other ABI version.
    pub(crate) unsafe fn read_vtable(ptr: *const EmbedVTable) -> Result<EmbedVTable, AbiMismatch> {
        // A guest built against a shorter table has fewer bytes than `EmbedVTable`, so copying the whole struct before the check would read past its end. `#[repr(C)]` puts `abi` at offset 0 for every version.
        let abi = unsafe { *ptr.cast::<u32>() };
        if abi != TELAR_EMBED_ABI {
            return Err(AbiMismatch { guest: abi });
        }
        Ok(unsafe { *ptr })
    }

    /// A guest built against another version of [`EmbedVTable`].
    #[derive(Debug, PartialEq, Eq)]
    pub(crate) struct AbiMismatch {
        pub(crate) guest: u32,
    }

    impl std::fmt::Display for AbiMismatch {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "guest built for ABI {}, host is ABI {TELAR_EMBED_ABI}",
                self.guest
            )
        }
    }

    impl LoadedEmbed {
        pub fn relayout(&self, width: f32, height: f32) {
            unsafe { (self.vtable.relayout)(self.inst, width, height) }
        }
        pub fn relayout_dirty(&self) {
            unsafe { (self.vtable.relayout_dirty)(self.inst) }
        }
        pub fn paint(&self) -> DrawList {
            unsafe { (self.vtable.paint)(self.inst) }
        }
        pub fn generation(&self) -> u64 {
            unsafe { (self.vtable.generation)(self.inst) }
        }
        pub fn on_event(&self, event: &Event) -> bool {
            unsafe { (self.vtable.on_event)(self.inst, event) }
        }
        pub fn dispatch_overlays(&self, event: &Event) -> bool {
            unsafe { (self.vtable.dispatch_overlays)(self.inst, event) }
        }
        /// Call once per frame the host drove this guest through, after its events. Closes the guest's one-frame keyboard state, which `key_pressed` inside it answers from.
        pub fn end_frame(&self) {
            unsafe { (self.vtable.end_frame)(self.inst) }
        }
        pub fn motion_tick(&self, now: Instant) {
            unsafe { (self.vtable.motion_tick)(self.inst, now) }
        }
        pub fn motion_active(&self) -> bool {
            unsafe { (self.vtable.motion_active)(self.inst) }
        }
        pub fn drain_window_commands(&self) -> WindowCommands {
            unsafe { (self.vtable.drain_window_commands)(self.inst) }
        }
        /// Hands the guest a new snapshot of the user's system preferences. [`load_embedded`] seeds it with the host's own; call this from the host's `on_system_preferences` so the guest follows later changes.
        pub fn set_system_preferences(&self, preferences: &SystemPreferences) {
            unsafe { (self.vtable.set_system_preferences)(self.inst, preferences) }
        }
        pub fn activate(&self) {
            unsafe { (self.vtable.activate)(self.inst) }
        }
        pub fn clear_color(&self) -> Option<Color> {
            unsafe { (self.vtable.clear_color)(self.inst) }
        }
        pub fn title(&self) -> String {
            unsafe { (self.vtable.title)(self.inst) }
        }
        pub fn icon(&self) -> Option<Vec<u8>> {
            unsafe { (self.vtable.icon)(self.inst) }
        }
        pub fn id(&self) -> String {
            unsafe { (self.vtable.id)(self.inst) }
        }
        pub fn on_frame(&self, ctx: &mut AppCtx) {
            unsafe { (self.vtable.on_frame)(self.inst, ctx) }
        }
    }

    impl Drop for LoadedEmbed {
        fn drop(&mut self) {
            // Runs dylib code touching its thread-locals, so it must happen before `_lib` unmaps.
            unsafe { (self.vtable.destroy)(self.inst) }
        }
    }
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
