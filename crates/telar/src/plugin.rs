//! Dynamic plugins: hosting a separately-`dlopen`'d rsx UI inside a host window.
//!
//! Unlike hot-reload (dev-only, one dylib that *replaces* the whole window), a plugin is a production
//! capability: the host stays a full rsx app and embeds one or more plugins, each a cdylib with its **own**
//! reactive/layout/overlay/motion runtime (separate thread-locals, because each dylib statically links its own
//! copy of the runtime crates). The host cannot reach into that runtime, so — exactly as hot-reload does for
//! its single dylib — it *drives* the plugin across the FFI boundary through exported shims.
//!
//! The novel part hot-reload never needed is **compositing two runtimes into one window**: the plugin flattens
//! its own view tree to a self-contained `Vec<DrawCommand>` ([`PluginInstance::paint`]) and hands it back; the
//! host translates + clips those commands into the plugin's sub-rect and splices them into its own frame. No
//! offscreen texture, no shared GPU device — the host's renderer paints everything in one pass.
//!
//! Layering: this module is app-agnostic. A plugin author implements [`EmbeddedApp`] (or an adapter to it) and
//! calls the [`plugin!`](crate::plugin) macro to export the shims. The host calls [`load_plugin`] and drives the
//! returned [`LoadedPlugin`]. Nothing here knows about any particular app.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use geometry_core::Rect;
use layout_core::AvailableSpace;
use platform_core::{Event, WindowCommand};
use renderer_core::{BorderRadius, Color, DrawCommand};
use ui_core::{ComponentList, EventResult, NodeId, Surface, compute_layout, mark_dirty};
use ui_tree::{Component, NodeVec, RenderNode};

use crate::app_context::AppCtx;

/// Owned draw-command list returned across the FFI boundary (the plugin's flattened frame). Self-contained:
/// baked geometry, `Arc`-shared styles/data — the host can render it directly (same-toolchain ABI, as with
/// hot-reload's `Vec<WindowCommand>`).
pub type DrawList = Vec<DrawCommand>;
/// Owned window-command list drained from the plugin's own queue (a title bar's drag/close etc.).
pub type WindowCommands = Vec<WindowCommand>;

// Re-exported for the `plugin!` macro's shim signatures, so a plugin crate needs only `$crate::plugin::*`.
pub use platform_core::Event as PluginEvent;
pub use renderer_core::Color as PluginColor;

/// Wrap a plugin's painted [`DrawList`] into a [`RenderNode`] the host splices into its own frame: translated
/// to `rect`'s origin (the plugin paints in its own `(0,0)` space) and clipped to `rect` (so it can't draw over
/// the host chrome). The exact idiom rsx's own scroll area uses to host a sub-tree in a viewport.
///
/// `image_salt` namespaces the plugin's image ids into a distinct range. Each dylib allocates `ImageData` ids
/// from its own process-local counter starting at 1, so a plugin's ids would otherwise alias the host's (and
/// other plugins') in the shared renderer's texture cache; a distinct nonzero salt per plugin instance keeps
/// them apart. Pass `0` to skip (single-runtime callers).
pub fn composite(rect: Rect, image_salt: u64, mut commands: DrawList) -> RenderNode {
    if image_salt != 0 {
        // Original ids are small and monotonic; shift the salt above them so `(salt, id)` stays unique and
        // stable frame-to-frame (so the texture cache still hits). `make_mut` clones only shared image data.
        const ID_BITS: u32 = 40;
        for cmd in &mut commands {
            if let DrawCommand::Image { data, .. } = cmd {
                let salted = (image_salt << ID_BITS) | (data.id & ((1u64 << ID_BITS) - 1));
                std::sync::Arc::make_mut(data).id = salted;
            }
        }
    }
    RenderNode::Clip {
        rect,
        radius: BorderRadius::zero(),
        children: NodeVec::collect([RenderNode::Transform {
            matrix: [1.0, 0.0, 0.0, 1.0, rect.x, rect.y],
            children: NodeVec::collect(commands.into_iter().map(RenderNode::Primitive)),
        }]),
    }
}

/// An embeddable rsx UI a host can drive as a plugin. The generic union of "build a view tree, render it,
/// handle events, run per-frame background work, and present a title/icon" — no app-specific semantics. A
/// concrete app (or an adapter over one) implements this; the [`plugin!`](crate::plugin) macro exports it.
///
/// Lifecycle the driver enforces: [`build`](Self::build) runs once, inside the plugin's freshly-entered
/// [`Surface`], so the content's layout nodes land in *this* surface's world; afterwards [`layout_root`] is the
/// node the driver sizes to the host's sub-rect.
pub trait EmbeddedApp: 'static {
    /// Build the content's layout tree. Called once by the driver with the plugin's surface active, so nodes
    /// are allocated in this surface's layout world. [`layout_root`](Self::layout_root) must be valid after it.
    fn build(&mut self);

    /// The content's top layout node — the one the driver `compute_layout`s to the host-assigned rect size.
    fn layout_root(&self) -> NodeId;

    /// Render the content to a [`RenderNode`] (flattened by the driver into the returned [`DrawList`]).
    fn view(&self) -> RenderNode;

    /// Route an event (already translated into the content's local coordinate space by the host).
    fn on_event(&mut self, event: &Event) -> EventResult;

    /// Re-lay-out internal scroll viewports after the driver has laid out the root at the new size. No-op for
    /// content without its own scroll roots.
    fn relayout_viewports(&mut self) {}

    /// Called when the content becomes visible (the host activated its tab). Autofocus the primary input here.
    fn activate(&mut self) {}

    /// Drain background-work channels into signals (see [`crate::RedrawWaker`]); the host forwards its `ctx`.
    fn on_frame(&mut self, _ctx: &mut AppCtx) {}

    /// The window/tab clear color, if the content wants one.
    fn clear_color(&self) -> Option<Color> {
        None
    }

    /// The plugin's display title (may read signals — the driver reads it with the surface active).
    fn title(&self) -> String;

    /// The plugin's icon bytes, owned so they cross the FFI boundary safely (no borrow into the dylib image).
    fn icon(&self) -> Option<Vec<u8>> {
        None
    }

    /// A stable identifier for this app kind (routing, discovery).
    fn id(&self) -> String;
}

/// Bridges the driver's [`ComponentList`] (which owns its root [`Component`]) to the shared [`EmbeddedApp`], so
/// the driver keeps its own handle to call `activate`/`relayout_viewports`/metadata while the segment tree
/// renders and dispatches events through the same object. Single-threaded; the borrows never overlap (paint
/// borrows during `commands()`, events during `on_event`, driver calls in between).
struct EmbeddedComponent(Rc<RefCell<Box<dyn EmbeddedApp>>>);

impl Component for EmbeddedComponent {
    fn view(&self) -> RenderNode {
        self.0.borrow().view()
    }
    fn on_event(&mut self, event: &Event) -> EventResult {
        self.0.borrow_mut().on_event(event)
    }
    fn debug_name(&self) -> &'static str {
        "PluginRoot"
    }
}

/// The dylib-side plugin driver: a headless single-surface runtime (no window, no renderer) that the host
/// drives across the FFI boundary. Owns the plugin's [`Surface`] and its [`ComponentList`]; every method
/// enters the surface first, so all work touches this plugin's thread-local worlds, not the host's.
///
/// The host holds this only as an opaque `*mut PluginInstance` (it never dereferences it — every call goes
/// through an exported shim so the code runs in the dylib). Constructed by [`__plugin_create`].
pub struct PluginInstance {
    embedded: Rc<RefCell<Box<dyn EmbeddedApp>>>,
    tree: ComponentList,
    root: NodeId,
    size: (f32, f32),
    task_waker_installed: bool,
    // Declared last so it drops last: the content and segment tree free their state while this surface's
    // worlds (layout, overlay, focus) still exist.
    surface: Rc<Surface>,
}

impl PluginInstance {
    /// Build the plugin: allocate its surface, build the content tree inside it, and mount the segment tree.
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

    /// Lay the content out to the host-assigned sub-rect size, then let it re-lay-out its own scroll viewports.
    /// `bump_force_ticks` re-runs every segment's view effect so it reads the fresh layout positions — the
    /// plugin's own reactive runtime doesn't track the host's layout writes, exactly like the hot-reload path.
    pub fn relayout(&mut self, width: f32, height: f32) {
        let _g = self.surface.enter();
        self.size = (width, height);
        let _ = mark_dirty(self.root);
        let _ = compute_layout(
            self.root,
            AvailableSpace::Definite(width),
            AvailableSpace::Definite(height),
        );
        // Batch so any signal the content writes here flushes AFTER the `borrow_mut` is released — otherwise the
        // synchronous flush re-runs the segment's `view()`, which borrows the same `RefCell` (double borrow).
        let embedded = &self.embedded;
        reactive_core::batch(|| embedded.borrow_mut().relayout_viewports());
        self.tree.bump_force_ticks();
    }

    /// Re-lay-out only what the plugin's own reactive changes dirtied (a list grew, a panel toggled), at the
    /// last size given to [`relayout`](Self::relayout). Driven every frame by the host — the analog of the
    /// runner calling `App::relayout` (`ui_core::relayout_if_dirty`) on an in-process app.
    pub fn relayout_dirty(&self) {
        let _g = self.surface.enter();
        ui_core::relayout_if_dirty();
    }

    /// The plugin's current frame as a flat, self-contained command list. The host translates it into the
    /// plugin's sub-rect and splices it into its own frame.
    pub fn paint(&self) -> DrawList {
        let _g = self.surface.enter();
        self.tree.commands().clone()
    }

    /// The content generation; unchanged between two reads means [`paint`](Self::paint) would return the same
    /// commands, so the host can skip re-fetching (mirrors the host renderer's idle-blit gate).
    pub fn generation(&self) -> u64 {
        let _g = self.surface.enter();
        self.tree.generation()
    }

    /// Whether an animation is still in flight in this plugin's motion engine.
    pub fn motion_active(&self) -> bool {
        let _g = self.surface.enter();
        motion_core::has_active()
    }

    /// Dispatch an event to the content (already in local coordinates). Self-batches in the plugin's runtime.
    pub fn on_event(&mut self, event: &Event) -> bool {
        let _g = self.surface.enter();
        self.tree.on_event(event) == EventResult::Handled
    }

    /// Route a positioned event to the plugin's overlay layer (modals/dropdowns) with priority; `true` means an
    /// overlay consumed it and the host should not fall through to the content.
    pub fn dispatch_overlays(&self, event: &Event) -> bool {
        let _g = self.surface.enter();
        // Batch so an overlay handler's signal writes flush after dispatch, not mid-walk (matches the runner's
        // event-batch bracket around overlay dispatch).
        reactive_core::batch(|| ui_core::dispatch_overlays(event) == EventResult::Handled)
    }

    /// Advance the plugin's motion engine and flush its runtime so animations progress and re-render.
    pub fn motion_tick(&self, now: Instant) {
        let _g = self.surface.enter();
        reactive_core::begin_batch();
        motion_core::tick(now);
        reactive_core::end_batch();
    }

    /// Drain window-management commands the plugin's UI enqueued (its title bar drag/minimize/close).
    pub fn drain_window_commands(&self) -> WindowCommands {
        let _g = self.surface.enter();
        platform_core::take_window_commands()
    }

    /// Write the OS light/dark preference into the plugin's theme runtime (drives its `follow_system`).
    pub fn set_system_dark(&self, dark: bool) {
        let _g = self.surface.enter();
        reactive_core::begin_batch();
        theme_core::set_system_dark(dark);
        reactive_core::end_batch();
    }

    /// Run the plugin's per-frame background-work hook, forwarding the host's `ctx` (so a plugin worker thread
    /// can wake the host loop via `ctx.redraw_waker()`, just as an in-process app does).
    pub fn on_frame(&mut self, ctx: &mut AppCtx) {
        let _g = self.surface.enter();
        // The plugin links its own reactive-core copy, so `spawn_task` inside it registers in a runtime the
        // host cannot reach. Both halves of the bridge are wired here rather than through new FFI symbols:
        // the host's wake goes in once, and the completions run on every frame it drives.
        if !self.task_waker_installed {
            if let Some(waker) = ctx.redraw_waker() {
                reactive_core::set_task_waker(move || waker.wake());
                self.task_waker_installed = true;
            }
        }
        reactive_core::drain_tasks();
        // Batch so signals the hook writes (draining channels) flush after the `borrow_mut` releases, never
        // re-entering `view()` mid-borrow (see `relayout`).
        let embedded = &self.embedded;
        reactive_core::batch(|| embedded.borrow_mut().on_frame(ctx));
    }

    /// Autofocus/announce the content becoming visible; re-render so a focus change shows this frame.
    pub fn activate(&mut self) {
        let _g = self.surface.enter();
        let embedded = &self.embedded;
        reactive_core::batch(|| embedded.borrow_mut().activate());
        self.tree.bump_force_ticks();
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

// --- Dylib-side shim helpers -------------------------------------------------------------------------------
// The `plugin!` macro exports one thin `#[no_mangle]` wrapper per method; each forwards to one of these so all
// logic stays here in rsx. `#[doc(hidden)]` — public only because macro expansion lands in the plugin crate.

impl Drop for PluginInstance {
    fn drop(&mut self) {
        // Background work this plugin started must not outlive it: its callbacks close over this surface's
        // state. Scoped to this instance's surface because two instances of the same plugin dylib share one
        // task registry (dlopen refcounts the library), so a blanket reset would cancel the sibling's work.
        reactive_core::cancel_tasks_for(self.surface.handle());
    }
}

/// Build a plugin instance and leak it to a raw pointer the host owns (freed via [`__plugin_destroy`]).
#[doc(hidden)]
pub fn __plugin_create(embedded: Box<dyn EmbeddedApp>) -> *mut PluginInstance {
    Box::into_raw(Box::new(PluginInstance::new(embedded)))
}

/// # Safety
/// `inst` must be a pointer returned by [`__plugin_create`] and not yet destroyed.
#[doc(hidden)]
pub unsafe fn __plugin_destroy(inst: *mut PluginInstance) {
    drop(unsafe { Box::from_raw(inst) });
}

macro_rules! plugin_shim {
    ($(#[$m:meta])* $vis_fn:ident ($($arg:ident : $ty:ty),*) $(-> $ret:ty)? => $method:ident) => {
        $(#[$m])*
        #[doc(hidden)]
        /// # Safety
        /// `inst` must be a live pointer from [`__plugin_create`].
        pub unsafe fn $vis_fn(inst: *mut PluginInstance $(, $arg: $ty)*) $(-> $ret)? {
            unsafe { (*inst).$method($($arg),*) }
        }
    };
}

plugin_shim!(__plugin_relayout(width: f32, height: f32) => relayout);
plugin_shim!(__plugin_relayout_dirty() => relayout_dirty);
plugin_shim!(__plugin_paint() -> DrawList => paint);
plugin_shim!(__plugin_generation() -> u64 => generation);
plugin_shim!(__plugin_on_event(event: &Event) -> bool => on_event);
plugin_shim!(__plugin_dispatch_overlays(event: &Event) -> bool => dispatch_overlays);
plugin_shim!(__plugin_motion_tick(now: Instant) => motion_tick);
plugin_shim!(__plugin_motion_active() -> bool => motion_active);
plugin_shim!(__plugin_drain_window_commands() -> WindowCommands => drain_window_commands);
plugin_shim!(__plugin_set_system_dark(dark: bool) => set_system_dark);
plugin_shim!(__plugin_activate() => activate);
plugin_shim!(__plugin_clear_color() -> Option<Color> => clear_color);
plugin_shim!(__plugin_title() -> String => title);
plugin_shim!(__plugin_icon() -> Option<Vec<u8>> => icon);
plugin_shim!(__plugin_id() -> String => id);

// on_frame takes `&mut AppCtx`, whose lifetime the shim macro can't spell; write it out.
/// # Safety
/// `inst` must be a live pointer from [`__plugin_create`].
#[doc(hidden)]
pub unsafe fn __plugin_on_frame(inst: *mut PluginInstance, ctx: &mut AppCtx) {
    unsafe { (*inst).on_frame(ctx) }
}

/// Exports the `_rsx_plugin_*` FFI symbols for a plugin cdylib. `$factory` is any
/// `Fn(&[String]) -> Box<dyn telar::EmbeddedApp>` — invoked once per instance with the launch args.
///
/// ```ignore
/// telar::plugin!(|args: &[String]| -> Box<dyn telar::EmbeddedApp> { Box::new(MyApp::new(args)) });
/// ```
///
/// The symbols are plain (release) exports — no `TELAR_HOT_RELOAD_BUILD`, no `dev` feature. They use the
/// `extern "Rust"` ABI, so a plugin must be built with the same toolchain as its host (a first-party plugin
/// model), exactly as hot-reload requires.
#[macro_export]
macro_rules! plugin {
    ($factory:expr) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_create(
            args: &[::std::string::String],
        ) -> *mut $crate::plugin::PluginInstance {
            $crate::plugin::__plugin_create(($factory)(args))
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_destroy(inst: *mut $crate::plugin::PluginInstance) {
            unsafe { $crate::plugin::__plugin_destroy(inst) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_relayout(
            inst: *mut $crate::plugin::PluginInstance,
            width: f32,
            height: f32,
        ) {
            unsafe { $crate::plugin::__plugin_relayout(inst, width, height) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_relayout_dirty(
            inst: *mut $crate::plugin::PluginInstance,
        ) {
            unsafe { $crate::plugin::__plugin_relayout_dirty(inst) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_paint(
            inst: *mut $crate::plugin::PluginInstance,
        ) -> $crate::plugin::DrawList {
            unsafe { $crate::plugin::__plugin_paint(inst) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_generation(
            inst: *mut $crate::plugin::PluginInstance,
        ) -> u64 {
            unsafe { $crate::plugin::__plugin_generation(inst) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_on_event(
            inst: *mut $crate::plugin::PluginInstance,
            event: &$crate::plugin::PluginEvent,
        ) -> bool {
            unsafe { $crate::plugin::__plugin_on_event(inst, event) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_dispatch_overlays(
            inst: *mut $crate::plugin::PluginInstance,
            event: &$crate::plugin::PluginEvent,
        ) -> bool {
            unsafe { $crate::plugin::__plugin_dispatch_overlays(inst, event) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_motion_tick(
            inst: *mut $crate::plugin::PluginInstance,
            now: ::std::time::Instant,
        ) {
            unsafe { $crate::plugin::__plugin_motion_tick(inst, now) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_motion_active(
            inst: *mut $crate::plugin::PluginInstance,
        ) -> bool {
            unsafe { $crate::plugin::__plugin_motion_active(inst) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_drain_window_commands(
            inst: *mut $crate::plugin::PluginInstance,
        ) -> $crate::plugin::WindowCommands {
            unsafe { $crate::plugin::__plugin_drain_window_commands(inst) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_set_system_dark(
            inst: *mut $crate::plugin::PluginInstance,
            dark: bool,
        ) {
            unsafe { $crate::plugin::__plugin_set_system_dark(inst, dark) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_activate(
            inst: *mut $crate::plugin::PluginInstance,
        ) {
            unsafe { $crate::plugin::__plugin_activate(inst) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_clear_color(
            inst: *mut $crate::plugin::PluginInstance,
        ) -> ::std::option::Option<$crate::plugin::PluginColor> {
            unsafe { $crate::plugin::__plugin_clear_color(inst) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_title(
            inst: *mut $crate::plugin::PluginInstance,
        ) -> ::std::string::String {
            unsafe { $crate::plugin::__plugin_title(inst) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_icon(
            inst: *mut $crate::plugin::PluginInstance,
        ) -> ::std::option::Option<::std::vec::Vec<u8>> {
            unsafe { $crate::plugin::__plugin_icon(inst) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_id(
            inst: *mut $crate::plugin::PluginInstance,
        ) -> ::std::string::String {
            unsafe { $crate::plugin::__plugin_id(inst) }
        }
        #[unsafe(no_mangle)]
        pub unsafe extern "Rust" fn _rsx_plugin_on_frame(
            inst: *mut $crate::plugin::PluginInstance,
            ctx: &mut $crate::AppCtx,
        ) {
            unsafe { $crate::plugin::__plugin_on_frame(inst, ctx) }
        }
    };
}

// --- Host-side loader --------------------------------------------------------------------------------------

#[cfg(feature = "plugin-host")]
pub use host::{LoadedPlugin, load_plugin};

#[cfg(feature = "plugin-host")]
mod host {
    use super::*;
    use std::path::Path;

    /// One resolved plugin export: the raw `extern "Rust"` function pointers, valid for as long as the owning
    /// `Library` stays mapped. Storing the pointers (rather than borrowed `Symbol`s) keeps [`LoadedPlugin`]
    /// non-self-referential and makes per-frame calls a plain indirect call.
    struct Symbols {
        destroy: unsafe extern "Rust" fn(*mut PluginInstance),
        relayout: unsafe extern "Rust" fn(*mut PluginInstance, f32, f32),
        relayout_dirty: unsafe extern "Rust" fn(*mut PluginInstance),
        paint: unsafe extern "Rust" fn(*mut PluginInstance) -> DrawList,
        generation: unsafe extern "Rust" fn(*mut PluginInstance) -> u64,
        on_event: unsafe extern "Rust" fn(*mut PluginInstance, &Event) -> bool,
        dispatch_overlays: unsafe extern "Rust" fn(*mut PluginInstance, &Event) -> bool,
        motion_tick: unsafe extern "Rust" fn(*mut PluginInstance, Instant),
        motion_active: unsafe extern "Rust" fn(*mut PluginInstance) -> bool,
        drain_window_commands: unsafe extern "Rust" fn(*mut PluginInstance) -> WindowCommands,
        set_system_dark: unsafe extern "Rust" fn(*mut PluginInstance, bool),
        activate: unsafe extern "Rust" fn(*mut PluginInstance),
        clear_color: unsafe extern "Rust" fn(*mut PluginInstance) -> Option<Color>,
        title: unsafe extern "Rust" fn(*mut PluginInstance) -> String,
        icon: unsafe extern "Rust" fn(*mut PluginInstance) -> Option<Vec<u8>>,
        id: unsafe extern "Rust" fn(*mut PluginInstance) -> String,
        on_frame: unsafe extern "Rust" fn(*mut PluginInstance, &mut AppCtx),
    }

    /// A loaded plugin the host drives. Holds the live instance (dylib-allocated) and the `Library` that must
    /// outlive it. `!Send`/`!Sync`: the instance is a foreign reactive runtime, driven only on the UI thread.
    pub struct LoadedPlugin {
        inst: *mut PluginInstance,
        symbols: Symbols,
        // Declared last so it drops last: the instance is destroyed (dylib code) before the library unmaps.
        _lib: libloading::Library,
    }

    /// Load a plugin cdylib and create one instance from it (calling its `_rsx_plugin_create` with `args`).
    ///
    /// The `.so` is loaded `RTLD_LOCAL` so its copies of the runtime symbols never interpose on the host's, and
    /// kept mapped for the plugin's lifetime — the instance holds live pointers into the dylib's code and data.
    pub fn load_plugin(
        path: &Path,
        args: &[String],
    ) -> Result<LoadedPlugin, Box<dyn std::error::Error>> {
        #[cfg(unix)]
        let lib = unsafe {
            libloading::os::unix::Library::open(
                Some(path.as_os_str()),
                libc::RTLD_NOW | libc::RTLD_LOCAL,
            )
            .map(libloading::Library::from)?
        };
        #[cfg(not(unix))]
        let lib = unsafe { libloading::Library::new(path)? };

        // Resolve every symbol once (deref the `Symbol` to copy out the raw fn pointer, valid while `lib` lives).
        let symbols = unsafe {
            Symbols {
                destroy: *lib.get(b"_rsx_plugin_destroy\0")?,
                relayout: *lib.get(b"_rsx_plugin_relayout\0")?,
                relayout_dirty: *lib.get(b"_rsx_plugin_relayout_dirty\0")?,
                paint: *lib.get(b"_rsx_plugin_paint\0")?,
                generation: *lib.get(b"_rsx_plugin_generation\0")?,
                on_event: *lib.get(b"_rsx_plugin_on_event\0")?,
                dispatch_overlays: *lib.get(b"_rsx_plugin_dispatch_overlays\0")?,
                motion_tick: *lib.get(b"_rsx_plugin_motion_tick\0")?,
                motion_active: *lib.get(b"_rsx_plugin_motion_active\0")?,
                drain_window_commands: *lib.get(b"_rsx_plugin_drain_window_commands\0")?,
                set_system_dark: *lib.get(b"_rsx_plugin_set_system_dark\0")?,
                activate: *lib.get(b"_rsx_plugin_activate\0")?,
                clear_color: *lib.get(b"_rsx_plugin_clear_color\0")?,
                title: *lib.get(b"_rsx_plugin_title\0")?,
                icon: *lib.get(b"_rsx_plugin_icon\0")?,
                id: *lib.get(b"_rsx_plugin_id\0")?,
                on_frame: *lib.get(b"_rsx_plugin_on_frame\0")?,
            }
        };
        let create: libloading::Symbol<unsafe extern "Rust" fn(&[String]) -> *mut PluginInstance> =
            unsafe { lib.get(b"_rsx_plugin_create\0")? };
        let inst = unsafe { create(args) };
        if inst.is_null() {
            return Err("plugin _rsx_plugin_create returned null".into());
        }
        Ok(LoadedPlugin {
            inst,
            symbols,
            _lib: lib,
        })
    }

    impl LoadedPlugin {
        pub fn relayout(&self, width: f32, height: f32) {
            unsafe { (self.symbols.relayout)(self.inst, width, height) }
        }
        pub fn relayout_dirty(&self) {
            unsafe { (self.symbols.relayout_dirty)(self.inst) }
        }
        pub fn paint(&self) -> DrawList {
            unsafe { (self.symbols.paint)(self.inst) }
        }
        pub fn generation(&self) -> u64 {
            unsafe { (self.symbols.generation)(self.inst) }
        }
        pub fn on_event(&self, event: &Event) -> bool {
            unsafe { (self.symbols.on_event)(self.inst, event) }
        }
        pub fn dispatch_overlays(&self, event: &Event) -> bool {
            unsafe { (self.symbols.dispatch_overlays)(self.inst, event) }
        }
        pub fn motion_tick(&self, now: Instant) {
            unsafe { (self.symbols.motion_tick)(self.inst, now) }
        }
        pub fn motion_active(&self) -> bool {
            unsafe { (self.symbols.motion_active)(self.inst) }
        }
        pub fn drain_window_commands(&self) -> WindowCommands {
            unsafe { (self.symbols.drain_window_commands)(self.inst) }
        }
        pub fn set_system_dark(&self, dark: bool) {
            unsafe { (self.symbols.set_system_dark)(self.inst, dark) }
        }
        pub fn activate(&self) {
            unsafe { (self.symbols.activate)(self.inst) }
        }
        pub fn clear_color(&self) -> Option<Color> {
            unsafe { (self.symbols.clear_color)(self.inst) }
        }
        pub fn title(&self) -> String {
            unsafe { (self.symbols.title)(self.inst) }
        }
        pub fn icon(&self) -> Option<Vec<u8>> {
            unsafe { (self.symbols.icon)(self.inst) }
        }
        pub fn id(&self) -> String {
            unsafe { (self.symbols.id)(self.inst) }
        }
        pub fn on_frame(&self, ctx: &mut AppCtx) {
            unsafe { (self.symbols.on_frame)(self.inst, ctx) }
        }
    }

    impl Drop for LoadedPlugin {
        fn drop(&mut self) {
            // Destroy the instance (runs dylib code touching its thread-locals) before `_lib` unmaps.
            unsafe { (self.symbols.destroy)(self.inst) }
        }
    }
}
