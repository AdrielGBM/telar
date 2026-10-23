//! [`HotApp`]: an application loaded from a dylib, and the exports the host drives its own runtime through.

mod boundary;
#[doc(hidden)]
pub mod guest;

use std::rc::Rc;

use boundary::check_guest;
pub use boundary::{GUEST_ABI, GUEST_LAYOUT};
use boundary::{ShieldedClipboard, ShieldedFileDialogs, ShieldedUriOpener};
use platform_core::guest::{BoundaryLayout, Guest};
pub use platform_core::guest::{GuestPanic, GuestResult};

use crate::app::App;
use crate::tree::{Frame, HotTree, UiTree};

/// The host's half of [`__hot_guest_api`](crate::__hot_guest_api): a table of every export, resolved once when the library loads.
macro_rules! host_exports {
    ($($field:ident => $symbol:ident($($arg:ident: $ty:ty),*) -> $output:ty;)*) => {
        /// Plain `fn` pointers copied out of the library, so the table borrows nothing; each is valid while the library stays mapped, which [`GuestImage`] guarantees by holding both.
        #[derive(Clone, Copy)]
        pub(crate) struct GuestExports {
            $(pub(crate) $field: unsafe extern "Rust" fn($($ty),*) -> $crate::hot::GuestResult<$output>,)*
        }

        impl GuestExports {
            fn resolve(lib: &libloading::Library) -> Result<Self, libloading::Error> {
                Ok(Self {
                    $($field: export(lib, concat!(stringify!($symbol), "\0").as_bytes())?,)*
                })
            }

            #[cfg(test)]
            pub(crate) fn in_process() -> Self {
                Self {
                    $($field: $crate::hot::guest::$field,)*
                }
            }
        }
    };
}

crate::__hot_guest_api!(host_exports);

/// The factory the `app!` macro writes into each dylib.
pub(crate) type CreateApp = unsafe extern "Rust" fn() -> GuestResult<Box<dyn App>>;

fn export<F: Copy>(lib: &libloading::Library, name: &[u8]) -> Result<F, libloading::Error> {
    unsafe { lib.get::<F>(name) }.map(|symbol| *symbol)
}

/// One loaded guest: its library, the exports resolved from it and the [`Guest`] every call goes through. Shared by the [`HotApp`] and every tree it mounted, so the guest is told it is unloading, and its library unmapped, only once the last of them lets go, whichever that is.
struct GuestImage {
    guest: Guest,
    exports: GuestExports,
    /// `None` for exports that are this process's own.
    lib: Option<libloading::Library>,
}

impl GuestImage {
    fn call<T>(&self, call: impl FnOnce(&GuestExports) -> GuestResult<T>) -> Option<T> {
        self.guest.call(|| call(&self.exports))
    }
}

impl Drop for GuestImage {
    // A guest that panicked is not told: `Guest::call` refuses it, since the unwind may have left its runtime half torn down.
    fn drop(&mut self) {
        let Some(lib) = self.lib.take() else {
            return;
        };
        self.call(|exports| unsafe { (exports.unloading)() });
        drop(lib);
    }
}

/// An application loaded from a dylib, driven through the exports it carries. Every call goes through its [`Guest`], so once the dylib has panicked none reaches it again; the panic is reported through [`AppRuntime::take_panic`](crate::AppRuntime::take_panic) and the host waits for a rebuild.
pub struct HotApp {
    /// `None` once a panic has left the dylib without one to give.
    app: Option<Box<dyn App>>,
    image: Rc<GuestImage>,
}

impl HotApp {
    /// Tells a guest loaded from a library that it will be unloaded, hands it the host's services, then has it build its app. A panic in any of them is recorded rather than returned: the host still has a window to keep, and a rebuild to wait for.
    pub(crate) fn start(
        exports: GuestExports,
        create: CreateApp,
        lib: Option<libloading::Library>,
    ) -> Self {
        let services = match lib {
            Some(_) => HostServices::installed(),
            None => HostServices::installed().in_this_process(),
        };
        let image = Rc::new(GuestImage {
            guest: Guest::default(),
            exports,
            lib,
        });
        if image.lib.is_some() {
            image.call(|exports| unsafe { (exports.loaded)() });
        }
        // Before the app is made, so whatever its setup or its first build reaches for is already there.
        image.call(|exports| unsafe { (exports.install_services)(services) });
        let app = image.call(|_| unsafe { create() });
        Self { app, image }
    }

    fn ask(&self, call: impl FnOnce(&GuestExports) -> GuestResult<bool>) -> bool {
        self.image.call(call).unwrap_or(false)
    }

    fn tell(&self, call: impl FnOnce(&GuestExports) -> GuestResult<()>) {
        self.image.call(call);
    }
}

impl Drop for HotApp {
    fn drop(&mut self) {
        let Some(app) = self.app.take() else {
            return;
        };
        if self.image.guest.has_panicked() {
            // Leaked: its destructor is code in a dylib that is no longer called.
            std::mem::forget(app);
            return;
        }
        self.tell(|exports| unsafe { (exports.app_release)(app) });
    }
}

/// The host's handle on a tree the dylib mounted and owns: an opaque pointer, and a share of the image whose code drives it, which keeps that image mapped for as long as the handle lives.
struct HotTreeHandle {
    ptr: *mut HotTree,
    image: Rc<GuestImage>,
}

impl UiTree for HotTreeHandle {
    fn on_event(&mut self, event: &platform_core::Event) -> ui_core::EventResult {
        let handled = self
            .image
            .call(|exports| unsafe { (exports.tree_on_event)(self.ptr, event) });
        if handled == Some(true) {
            ui_core::EventResult::Handled
        } else {
            ui_core::EventResult::Ignored
        }
    }

    fn on_covered_event(&mut self, event: &platform_core::Event) {
        self.image
            .call(|exports| unsafe { (exports.tree_on_covered_event)(self.ptr, event) });
    }

    fn frame(&self) -> Frame<'_> {
        let commands = self
            .image
            .call(|exports| unsafe { (exports.tree_paint)(self.ptr) });
        Frame::Owned(commands.unwrap_or_default())
    }

    fn end_frame(&self) {
        self.image
            .call(|exports| unsafe { (exports.tree_end_frame)(self.ptr) });
    }

    fn is_dirty(&self) -> bool {
        self.image
            .call(|exports| unsafe { (exports.tree_dirty)(self.ptr) })
            .unwrap_or(false)
    }

    fn generation(&self) -> u64 {
        self.image
            .call(|exports| unsafe { (exports.tree_generation)(self.ptr) })
            .unwrap_or_default()
    }

    fn walk(&self, out: &mut Vec<ui_tree::SegmentNodeInfo>) {
        if let Some(nodes) = self
            .image
            .call(|exports| unsafe { (exports.tree_walk)(self.ptr) })
        {
            out.extend(nodes);
        }
    }
}

impl Drop for HotTreeHandle {
    // A dylib that panicked keeps the tree: releasing it would run that dylib's code again.
    fn drop(&mut self) {
        self.image
            .call(|exports| unsafe { (exports.tree_release)(self.ptr) });
    }
}

/// What the runner drives while there is no tree to: the dylib panicked before it could mount one.
struct NoTree;

impl UiTree for NoTree {
    fn on_event(&mut self, _event: &platform_core::Event) -> ui_core::EventResult {
        ui_core::EventResult::Ignored
    }

    fn on_covered_event(&mut self, _event: &platform_core::Event) {}

    fn frame(&self) -> Frame<'_> {
        Frame::Owned(Vec::new())
    }

    fn is_dirty(&self) -> bool {
        false
    }

    fn generation(&self) -> u64 {
        0
    }

    fn walk(&self, _out: &mut Vec<ui_tree::SegmentNodeInfo>) {}
}

impl crate::app_runtime::AppRuntime for HotApp {
    fn window_config(&self) -> Option<platform_core::WindowConfig> {
        let app = self.app.as_deref()?;
        self.image
            .call(|exports| unsafe { (exports.app_window_config)(app) })
            .flatten()
    }

    // Mounted inside the dylib, where the app's signals live: a tree mounted out here would register its segment effects in the host's runtime and never subscribe to anything the app writes.
    fn mount(&mut self) -> Box<dyn UiTree> {
        let Some(app) = self.app.as_deref() else {
            return Box::new(NoTree);
        };
        let mounted = self
            .image
            .call(|exports| unsafe { (exports.tree_mount)(app) });
        match mounted {
            Some(ptr) => Box::new(HotTreeHandle {
                ptr,
                image: Rc::clone(&self.image),
            }),
            None => Box::new(NoTree),
        }
    }

    fn clear_color(&self) -> Option<renderer_core::Color> {
        let app = self.app.as_deref()?;
        self.image
            .call(|exports| unsafe { (exports.app_clear_color)(app) })
            .flatten()
    }

    // The waker in `ctx` is host code the app may call at any time, so the app is handed one that keeps a panic on this side.
    fn on_frame(&mut self, ctx: &mut platform_core::AppCtx) {
        let Some(app) = self.app.as_deref_mut() else {
            return;
        };
        platform_core::guest::with_guest_ctx(ctx, |guest_ctx| {
            self.image
                .call(|exports| unsafe { (exports.app_on_frame)(app, guest_ctx) })
        });
    }

    fn take_panic(&self) -> Option<String> {
        self.image.guest.take_panic().map(|panic| panic.to_string())
    }

    fn hot_snapshot(&self) -> Option<String> {
        self.image.call(|exports| unsafe { (exports.snapshot)() })
    }

    fn hot_restore(&self, blob: &str) {
        self.tell(|exports| unsafe { (exports.restore)(blob) });
    }

    // The app's motion registry is the dylib's motion-core copy; the host's animates what the host draws itself, the devtools overlay.
    fn motion_tick(&self, now: web_time::Instant) {
        motion_core::tick(now);
        self.tell(|exports| unsafe { (exports.motion_tick)(now) });
    }

    fn motion_has_active(&self) -> bool {
        motion_core::has_active() || self.ask(|exports| unsafe { (exports.motion_active)() })
    }

    fn motion_has_continuous(&self) -> bool {
        motion_core::has_continuous()
            || self.ask(|exports| unsafe { (exports.motion_continuous)() })
    }

    fn begin_event_batch(&self) {
        self.tell(|exports| unsafe { (exports.begin_batch)() });
    }

    fn end_event_batch(&self) {
        self.tell(|exports| unsafe { (exports.end_batch)() });
    }

    fn relayout(&self) {
        self.tell(|exports| unsafe { (exports.relayout)() });
    }

    // `overlay` widgets register in the dylib where the view is built, so a modal's priority routing must be driven across this boundary.
    fn dispatch_overlays(&self, event: &platform_core::Event) -> bool {
        self.ask(|exports| unsafe { (exports.dispatch_overlays)(event) })
    }

    // The controls, their names and their rects live in the dylib's registries, which no host-side reading can see.
    fn access_snapshot(
        &self,
        frame: &[renderer_core::DrawCommand],
    ) -> Vec<platform_core::AccessNode> {
        self.access_audit(frame)
            .map(|audit| audit.nodes)
            .unwrap_or_default()
    }

    fn access_audit(
        &self,
        frame: &[renderer_core::DrawCommand],
    ) -> Option<platform_core::AccessAudit> {
        let background = self.clear_color();
        self.image
            .call(|exports| unsafe { (exports.access_snapshot)(frame, background) })
    }

    // A title bar's `on_press` pushes into the dylib's platform-core copy, so the host drains it across this boundary.
    fn drain_window_commands(&self) -> Vec<platform_core::WindowCommand> {
        self.image
            .call(|exports| unsafe { (exports.drain_window_commands)() })
            .unwrap_or_default()
    }

    // The field and its caret live in the dylib's focus and layout registries.
    fn text_input(&self) -> Option<platform_core::TextInput> {
        self.image
            .call(|exports| unsafe { (exports.text_input)() })
            .flatten()
    }

    // The dylib reads its own copy of the store; the host's copy is kept too, for the devtools that live on this side.
    fn set_system_preferences(&self, preferences: &platform_core::SystemPreferences) {
        preferences_core::set_system_preferences(preferences.clone());
        self.tell(|exports| unsafe { (exports.set_system_preferences)(preferences) });
    }

    fn set_surface_size(&self, size: geometry_core::Size) {
        self.tell(|exports| unsafe { (exports.set_surface_size)(size.width, size.height) });
    }

    fn set_safe_area_insets(&self, insets: geometry_core::Insets) {
        self.tell(|exports| unsafe {
            (exports.set_safe_area_insets)(insets.top, insets.right, insets.bottom, insets.left)
        });
    }

    fn set_surface_maximized(&self, maximized: bool) {
        self.tell(|exports| unsafe { (exports.set_surface_maximized)(maximized) });
    }

    fn set_location_history(&self, history: &[platform_core::Location]) {
        self.tell(|exports| unsafe { (exports.set_location_history)(history) });
    }

    fn open_title(&self, app: &str, showing: &str) {
        self.tell(|exports| unsafe { (exports.open_title)(app, showing) });
    }

    fn open_font_family(&self, family: Option<&str>) {
        self.tell(|exports| unsafe { (exports.open_font_family)(family) });
    }

    // Its dialogs and its history are the dylib's.
    fn navigate_back(&self) -> bool {
        self.ask(|exports| unsafe { (exports.navigate_back)() })
    }

    // `spawn_task` registers its callback in the reactive-core copy of whoever called it, so both sides are drained.
    fn drain_tasks(&self) {
        reactive_core::drain_tasks();
        self.tell(|exports| unsafe { (exports.drain_tasks)() });
    }

    fn fire_timers(&self) {
        reactive_core::fire_timers();
        self.tell(|exports| unsafe { (exports.fire_timers)() });
    }

    // The loop wakes for whichever side's timer comes due first.
    fn until_next_timer(&self) -> Option<std::time::Duration> {
        let host = reactive_core::until_next_timer();
        let app = self
            .image
            .call(|exports| unsafe { (exports.until_next_timer)() })
            .flatten();
        match (host, app) {
            (Some(host), Some(app)) => Some(host.min(app)),
            (host, app) => host.or(app),
        }
    }

    // The dylib wakes the loop from whatever thread finishes its task, so what it is handed keeps a panic in the wake on this side.
    fn install_task_waker(&self, waker: platform_core::RedrawWaker) {
        let host = waker.clone();
        reactive_core::set_task_waker(move || host.wake());
        let waker = platform_core::guest::shielded_waker(waker);
        self.tell(move |exports| unsafe { (exports.install_task_waker)(waker) });
    }
}

/// The library a build left at `path`, opened and started. An error means the library could not be driven at all; a panic while it starts is recorded in the [`HotApp`] instead.
pub fn load_hot_app(path: &std::path::Path) -> Result<HotApp, Box<dyn std::error::Error>> {
    let lib = platform_core::guest::open_unique(path)?;
    let abi = export::<unsafe extern "Rust" fn() -> u32>(&lib, b"_rsx_hot_abi\0")
        .map(|abi| unsafe { abi() })
        .ok();
    let layout = export::<unsafe extern "Rust" fn() -> u64>(&lib, b"_rsx_hot_layout\0")
        .map(|layout| BoundaryLayout(unsafe { layout() }))
        .ok();
    check_guest(abi, layout).map_err(|why| format!("{}: {why}", path.display()))?;
    let exports = GuestExports::resolve(&lib)?;
    let create = export::<CreateApp>(&lib, b"_rsx_hot_create_app\0")?;
    Ok(HotApp::start(exports, create, Some(lib)))
}

/// The platform services the host installed, handed to each library it loads: the library's copies of their stores start empty, so without them a reloaded app copies to no clipboard, opens no link, shows no file dialog, lays its text out in faces other than the ones the host draws it in, and shows and matches its shortcuts for the keyboard it was compiled for rather than the one the host's platform reports.
///
/// The host's backends serve both sides, so a selection the clipboard owns outlives every library swapped in under it. In a build that shapes its own text the host's font database is handed over too, for the library's copy of the shaper to defer to: two copies of the shaper cannot share one database, so the library loads what the host's was loaded from, and a face the app adds while it runs is added to the host's, which draws its text, before the library loads it as well.
#[cfg(feature = "dev")]
#[doc(hidden)]
pub struct HostServices {
    clipboard: Option<std::sync::Arc<dyn services_core::Clipboard>>,
    uri_opener: Option<std::sync::Arc<dyn services_core::UriOpener>>,
    file_dialogs: Option<std::sync::Arc<dyn services_core::FileDialogs>>,
    keyboard_convention: platform_core::KeyboardConvention,
    fonts: Option<std::sync::Arc<dyn renderer_core::FontHost>>,
}

#[cfg(feature = "dev")]
impl HostServices {
    /// What this side has installed, each backend wrapped so a panic in it stays on this side of the boundary.
    pub fn installed() -> Self {
        Self {
            clipboard: services_core::clipboard().map(|clipboard| {
                std::sync::Arc::new(ShieldedClipboard(clipboard))
                    as std::sync::Arc<dyn services_core::Clipboard>
            }),
            uri_opener: services_core::uri_opener().map(|opener| {
                std::sync::Arc::new(ShieldedUriOpener(opener))
                    as std::sync::Arc<dyn services_core::UriOpener>
            }),
            file_dialogs: services_core::file_dialogs().map(|dialogs| {
                std::sync::Arc::new(ShieldedFileDialogs(dialogs))
                    as std::sync::Arc<dyn services_core::FileDialogs>
            }),
            keyboard_convention: platform_core::keyboard_convention(),
            fonts: served_fonts(),
        }
    }

    /// These services for exports that are this process's own, which already set text in its one font database: handed it, they would defer it to itself.
    fn in_this_process(self) -> Self {
        Self {
            fonts: None,
            ..self
        }
    }

    /// Installs each service on this side, as the host had it.
    pub fn install(self) {
        let Self {
            clipboard,
            uri_opener,
            file_dialogs,
            keyboard_convention,
            fonts,
        } = self;
        platform_core::set_keyboard_convention(keyboard_convention);
        #[cfg(feature = "shaper")]
        if let Some(fonts) = fonts {
            renderer_text::fonts::defer_to(fonts);
        }
        #[cfg(not(feature = "shaper"))]
        drop(fonts);
        if let Some(clipboard) = clipboard {
            services_core::set_clipboard(clipboard);
        }
        if let Some(opener) = uri_opener {
            services_core::set_uri_opener(opener);
        }
        if let Some(dialogs) = file_dialogs {
            services_core::set_file_dialogs(dialogs);
        }
    }
}

#[cfg(all(feature = "dev", feature = "shaper"))]
fn served_fonts() -> Option<std::sync::Arc<dyn renderer_core::FontHost>> {
    Some(std::sync::Arc::new(renderer_text::fonts::Served))
}

#[cfg(all(feature = "dev", not(feature = "shaper")))]
fn served_fonts() -> Option<std::sync::Arc<dyn renderer_core::FontHost>> {
    None
}

#[cfg(feature = "dev")]
use telar_project::protocol::{BUILD_ERROR_PREFIX, FOCUS_LINE, GOTO_PREFIX, HOT_RELOAD_PREFIX};

#[cfg(feature = "dev")]
/// What `cargo telar dev` sends the running app: a rebuild landed, a build failed, a location to open, or a request to come forward.
#[derive(Debug, PartialEq)]
pub enum HotEvent {
    Reload(std::path::PathBuf),
    BuildError(String),
    /// A location reference, as `LocationFormat::parse` reads one, to open as a link into the running app would: `cargo telar preview <ID>` moving the workshop already open.
    Goto(String),
    /// `cargo telar preview` finding the workshop already open: its window comes forward, showing what it was.
    Focus,
}

/// The event one line of the channel carries, or `None` for a line with a prefix this host does not know.
#[cfg(feature = "dev")]
fn hot_event(line: &str) -> Option<HotEvent> {
    if let Some(path) = line.strip_prefix(HOT_RELOAD_PREFIX) {
        Some(HotEvent::Reload(std::path::PathBuf::from(path)))
    } else if let Some(message) = line.strip_prefix(BUILD_ERROR_PREFIX) {
        Some(HotEvent::BuildError(unescape_lines(message)))
    } else if line == FOCUS_LINE {
        Some(HotEvent::Focus)
    } else {
        line.strip_prefix(GOTO_PREFIX)
            .map(|reference| HotEvent::Goto(reference.to_string()))
    }
}

/// Connects to the cargo-telar TCP loopback channel (it binds the port and passes it via `TELAR_HOT_PORT`) and forwards line-delimited hot events. TCP instead of a unix socket so the same code path works on non-Unix hosts.
#[cfg(feature = "dev")]
pub fn listen_hot_reload(port: u16) -> std::sync::mpsc::Receiver<HotEvent> {
    use std::io::BufRead;
    use std::net::TcpStream;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("telar-hot-reload".to_string())
        .spawn(move || {
            // cargo-telar binds before spawning us, but retry briefly in case it is mid-rebuild.
            let mut stream = None;
            for _ in 0..20 {
                match TcpStream::connect(("127.0.0.1", port)) {
                    Ok(s) => {
                        stream = Some(s);
                        break;
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(250)),
                }
            }
            let Some(stream) = stream else {
                tracing::error!("hot reload channel connect failed (port {port})");
                return;
            };
            let reader = std::io::BufReader::new(stream);
            for line in reader.lines() {
                let Ok(line) = line else { break };
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Some(event) = hot_event(line) else {
                    continue;
                };
                if tx.send(event).is_err() {
                    break;
                }
            }
        })
        .ok();
    rx
}

/// Undoes the escaping `cargo-telar` applies so a multi-line build error survives a protocol of one event per line. A trailing lone backslash cannot occur (the sender doubles them) and is passed through rather than dropped, so a malformed message is still shown.
fn unescape_lines(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut chars = message.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
#[path = "hot_test.rs"]
mod tests;
