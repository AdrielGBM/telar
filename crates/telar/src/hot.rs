//! [`HotApp`]: an application loaded from a dylib, and the entries the host drives its own runtime through.

mod entries;
#[doc(hidden)]
pub mod guest;

use std::cell::RefCell;
use std::rc::{Rc, Weak};

#[doc(hidden)]
pub use entries::{CreateApp, HOT_CONTRACT, HotEntries};
#[doc(hidden)]
pub use platform_core::export_contract_stamp;
pub use platform_core::guest::GuestPanic;
use platform_core::guest::{CWaker, FrameCtx, LentBytes};
#[doc(hidden)]
pub use platform_core::guest::{CallOutcome, Handle, HostServices};
use renderer_core::wire::FrameDecoder;
use services_core::guest::GuestLibrary;
use ui_core::{ForeignBoundary, ForeignSurface, GuestSurface};
use wire_core::encode;

use crate::tree::{Frame, HotTree, UiTree};
use guest::GuestApp;

/// One loaded guest and the surface of the tree whose frame was read last, the window's. Shared by the [`HotApp`] and every tree it mounted, and the library with the surface of each, so the guest is unloaded only once the last of them lets go, whichever that is.
struct HotImage {
    library: Rc<GuestLibrary<HotEntries>>,
    painted: RefCell<Weak<GuestSurface<HotEntries>>>,
}

/// An application loaded from a dylib, driven through the entries it exports. Every call goes through its [`GuestLibrary`], so once the dylib has panicked none reaches it again; the panic is reported through [`AppRuntime::take_panic`](crate::AppRuntime::take_panic) and the host waits for a rebuild.
pub struct HotApp {
    /// `None` once a panic has left the dylib without one to give.
    app: Option<Handle<GuestApp>>,
    image: Rc<HotImage>,
}

impl HotApp {
    /// Starts the guest `entries` were copied from and has it build its app; `library` is its mapping, `None` for entries this process serves itself. A panic in any of it is recorded rather than returned: the host still has a window to keep, and a rebuild to wait for.
    pub(crate) fn start(entries: HotEntries, library: Option<libloading::Library>) -> Self {
        let library = GuestLibrary::start(
            entries,
            library.map(|library| Box::new(library) as Box<dyn std::any::Any>),
            served_fonts(),
        );
        let app = library.answer(Handle::NULL, |entries, app| unsafe {
            (entries.create)(app)
        });
        Self {
            app,
            image: Rc::new(HotImage {
                library: Rc::new(library),
                painted: RefCell::default(),
            }),
        }
    }

    fn library(&self) -> &GuestLibrary<HotEntries> {
        &self.image.library
    }
}

// The frame an inspector reads is the one the tree painted last, which the guest still holds: audited there, it never has to be spelled back across, pixels and all.
fn guest_audit(
    library: &GuestLibrary<HotEntries>,
    tree: Handle<HotTree>,
    background: Option<renderer_core::Color>,
) -> Option<platform_core::AccessAudit> {
    let background = encode(&background);
    library.read(|entries, audit| unsafe {
        (entries.tree_access_audit)(tree, LentBytes::lend(&background), audit)
    })
}

impl Drop for HotApp {
    fn drop(&mut self) {
        if let Some(app) = self.app.take() {
            self.library()
                .call(|entries| unsafe { (entries.release)(app) });
        }
    }
}

/// The host's handle on a tree the dylib mounted and owns: an opaque handle, the decoder its frames are read back with, which keeps the payloads and images consecutive frames share, what the tree reports of the window it fills, the boundary its frame is marked with, and a share of the image whose code drives it, which keeps that image mapped for as long as the handle lives.
///
/// The frame it hands the runner is marked as the guest's whole ([`ForeignBoundary::marked`]): the runner's screen reader and its inspector read the window as the tree reports it, and a reader's action on it reaches the tree.
struct HotTreeHandle {
    tree: Handle<HotTree>,
    frames: RefCell<FrameDecoder>,
    surface: Rc<GuestSurface<HotEntries>>,
    boundary: ForeignBoundary,
    image: Rc<HotImage>,
}

impl HotTreeHandle {
    fn library(&self) -> &GuestLibrary<HotEntries> {
        &self.image.library
    }
}

impl UiTree for HotTreeHandle {
    fn on_event(&mut self, event: &platform_core::Event) -> ui_core::EventResult {
        let tree = self.tree;
        let event = match event {
            platform_core::Event::InSurface { .. } => match self.boundary.foreign_event(event) {
                Some(event) => encode(&event),
                None => return ui_core::EventResult::Ignored,
            },
            event => encode(event),
        };
        let handled = self.library().ask(|entries, handled| unsafe {
            (entries.tree_on_event)(tree, LentBytes::lend(&event), handled)
        });
        if handled {
            ui_core::EventResult::Handled
        } else {
            ui_core::EventResult::Ignored
        }
    }

    fn on_covered_event(&mut self, event: &platform_core::Event) {
        let tree = self.tree;
        self.library().send(event, |entries, event| unsafe {
            (entries.tree_on_covered_event)(tree, event)
        });
    }

    fn frame(&self) -> Frame<'_> {
        let tree = self.tree;
        *self.image.painted.borrow_mut() = Rc::downgrade(&self.surface);
        Frame::Owned(self.boundary.marked(self.library().read_frame(
            &mut self.frames.borrow_mut(),
            |entries, frame| unsafe { (entries.tree_paint)(tree, frame) },
            |entries, image, pixels, found| unsafe {
                (entries.tree_pixels)(tree, image, pixels, found)
            },
            |entries| unsafe { (entries.tree_reset_frames)(tree) },
        )))
    }

    fn end_frame(&self) {
        let tree = self.tree;
        self.library()
            .call(|entries| unsafe { (entries.tree_end_frame)(tree) });
    }

    fn is_dirty(&self) -> bool {
        let tree = self.tree;
        self.library()
            .ask(|entries, dirty| unsafe { (entries.tree_is_dirty)(tree, dirty) })
    }

    fn generation(&self) -> u64 {
        let tree = self.tree;
        self.library()
            .answer(0, |entries, generation| unsafe {
                (entries.tree_generation)(tree, generation)
            })
            .unwrap_or_default()
    }

    fn walk(&self, out: &mut Vec<ui_tree::SegmentNodeInfo>) {
        let tree = self.tree;
        let nodes: Option<Vec<ui_tree::SegmentNodeInfo>> = self
            .library()
            .read(|entries, nodes| unsafe { (entries.tree_walk)(tree, nodes) });
        out.extend(nodes.into_iter().flatten());
    }
}

impl Drop for HotTreeHandle {
    // A dylib that panicked keeps the tree: releasing it would run that dylib's code again.
    fn drop(&mut self) {
        let tree = self.tree;
        self.surface.forget();
        self.library()
            .call(|entries| unsafe { (entries.tree_release)(tree) });
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
        let app = self.app?;
        self.library()
            .read(|entries, config| unsafe { (entries.window_config)(app, config) })
            .flatten()
    }

    // Mounted inside the dylib, where the app's signals live: a tree mounted out here would register its segment effects in the host's runtime and never subscribe to anything the app writes.
    fn mount(&mut self) -> Box<dyn UiTree> {
        let Some(app) = self.app else {
            return Box::new(NoTree);
        };
        let mounted = self.library().answer(Handle::NULL, |entries, tree| unsafe {
            (entries.mount)(app, tree)
        });
        let Some(tree) = mounted else {
            return Box::new(NoTree);
        };
        let surface = Rc::new(
            GuestSurface::new(Rc::clone(&self.image.library), tree).audited_by(guest_audit),
        );
        let source: Rc<dyn ForeignSurface> = surface.clone();
        Box::new(HotTreeHandle {
            tree,
            frames: RefCell::default(),
            boundary: ForeignBoundary::new(&source),
            surface,
            image: Rc::clone(&self.image),
        })
    }

    fn clear_color(&self) -> Option<renderer_core::Color> {
        let app = self.app?;
        self.library()
            .read(|entries, color| unsafe { (entries.clear_color)(app, color) })
            .flatten()
    }

    fn on_frame(&mut self, ctx: &mut platform_core::AppCtx) {
        let Some(app) = self.app else {
            return;
        };
        FrameCtx::lend(ctx, |frame| {
            self.library()
                .call(|entries| unsafe { (entries.on_frame)(app, frame) })
        });
    }

    fn take_panic(&self) -> Option<String> {
        self.library()
            .guest()
            .take_panic()
            .map(|panic| panic.to_string())
    }

    fn hot_snapshot(&self) -> Option<String> {
        self.library()
            .read(|entries, blob| unsafe { (entries.hot_snapshot)(blob) })
    }

    fn hot_restore(&self, blob: &str) {
        self.library()
            .send(&blob.to_owned(), |entries, blob| unsafe {
                (entries.hot_restore)(blob)
            });
    }

    // The app's motion registry is the dylib's motion-core copy; the host's animates what the host draws itself, the devtools overlay.
    fn motion_tick(&self, now: web_time::Instant) {
        motion_core::tick(now);
        let now = self.library().epoch().stamp(now);
        self.library()
            .call(|entries| unsafe { (entries.motion_tick)(now) });
    }

    fn motion_has_active(&self) -> bool {
        motion_core::has_active()
            || self
                .library()
                .ask(|entries, active| unsafe { (entries.motion_has_active)(active) })
    }

    fn motion_has_continuous(&self) -> bool {
        motion_core::has_continuous()
            || self
                .library()
                .ask(|entries, continuous| unsafe { (entries.motion_has_continuous)(continuous) })
    }

    // Ended by a guard, so an unwind out of the dispatch cannot leave the dylib's batch open.
    fn in_event_batch(&self, dispatch: &mut dyn FnMut()) {
        struct EndBatch<'a>(&'a GuestLibrary<HotEntries>);
        impl Drop for EndBatch<'_> {
            fn drop(&mut self) {
                self.0.call(|entries| unsafe { (entries.end_batch)() });
            }
        }
        self.library()
            .call(|entries| unsafe { (entries.begin_batch)() });
        let _end = EndBatch(self.library());
        dispatch();
    }

    fn relayout(&self) {
        self.library()
            .call(|entries| unsafe { (entries.relayout)() });
    }

    // `overlay` widgets register in the dylib where the view is built, so a modal's priority routing must be driven across this boundary.
    fn dispatch_overlays(&self, event: &platform_core::Event) -> bool {
        let event = encode(event);
        self.library().ask(|entries, handled| unsafe {
            (entries.dispatch_overlays)(LentBytes::lend(&event), handled)
        })
    }

    // A title bar's `on_press` pushes into the dylib's platform-core copy, so the host drains it across this boundary.
    fn drain_window_commands(&self) -> Vec<platform_core::WindowCommand> {
        self.library()
            .read(|entries, commands| unsafe { (entries.drain_window_commands)(commands) })
            .unwrap_or_default()
    }

    // The field and its caret live in the dylib's focus and layout registries, which the tree that fills the window reports, in the window's coordinates.
    fn text_input(&self) -> Option<platform_core::TextInput> {
        self.image.painted.borrow().upgrade()?.text_input()
    }

    // The dylib reads its own copy of the store; the host's copy is kept too, for the devtools that live on this side.
    fn set_system_preferences(&self, preferences: &platform_core::SystemPreferences) {
        preferences_core::set_system_preferences(preferences.clone());
        self.library()
            .send(preferences, |entries, preferences| unsafe {
                (entries.set_system_preferences)(preferences)
            });
    }

    fn set_surface_size(&self, size: geometry_core::Size) {
        self.library()
            .call(|entries| unsafe { (entries.set_surface_size)(size.width, size.height) });
    }

    fn set_safe_area_insets(&self, insets: geometry_core::Insets) {
        self.library().call(|entries| unsafe {
            (entries.set_safe_area_insets)(insets.top, insets.right, insets.bottom, insets.left)
        });
    }

    fn set_surface_maximized(&self, maximized: bool) {
        self.library()
            .call(|entries| unsafe { (entries.set_surface_maximized)(maximized) });
    }

    fn set_location_history(&self, history: &[platform_core::Location]) {
        self.library()
            .send(&history.to_vec(), |entries, history| unsafe {
                (entries.set_location_history)(history)
            });
    }

    fn open_title(&self, app: &str, showing: &str) {
        self.library().send(
            &(app.to_owned(), showing.to_owned()),
            |entries, title| unsafe { (entries.open_title)(title) },
        );
    }

    fn open_font_family(&self, family: Option<&str>) {
        self.library()
            .send(&family.map(str::to_owned), |entries, family| unsafe {
                (entries.open_font_family)(family)
            });
    }

    // Its dialogs and its history are the dylib's.
    fn navigate_back(&self) -> bool {
        self.library()
            .ask(|entries, handled| unsafe { (entries.navigate_back)(handled) })
    }

    // `spawn_task` registers its callback in the reactive-core copy of whoever called it, so both sides are drained.
    fn drain_tasks(&self) {
        reactive_core::drain_tasks();
        self.library()
            .call(|entries| unsafe { (entries.drain_tasks)() });
    }

    fn fire_timers(&self) {
        reactive_core::fire_timers();
        self.library()
            .call(|entries| unsafe { (entries.fire_timers)() });
    }

    // The loop wakes for whichever side's timer comes due first.
    fn until_next_timer(&self) -> Option<std::time::Duration> {
        let host = reactive_core::until_next_timer();
        let app = self
            .library()
            .read(|entries, after| unsafe { (entries.until_next_timer)(after) })
            .flatten();
        match (host, app) {
            (Some(host), Some(app)) => Some(host.min(app)),
            (host, app) => host.or(app),
        }
    }

    // The dylib wakes the loop from whatever thread finishes its task, through a reference to the host's waker whose wake keeps a panic on this side.
    fn install_task_waker(&self, waker: platform_core::RedrawWaker) {
        let host = waker.clone();
        reactive_core::set_task_waker(move || host.wake());
        let waker = CWaker::new(waker);
        self.library()
            .call(move |entries| unsafe { (entries.install_task_waker)(waker) });
    }
}

/// The library a build left at `path`, opened and started. An error means the library could not be driven at all, and names the library and why: one built against another telar, another kind of guest or no telar guest at all is refused before anything of it runs but its initializers. A panic while it starts is recorded in the [`HotApp`] instead.
///
/// # Safety
///
/// As [`open_guest`](platform_core::guest::open_guest): `path` must name a hot-reload library built from a telar this host trusts. Loading runs its initializers, and once its [`HOT_CONTRACT`] matches, its [`HotEntries`] are called as they are, foreign code in this process.
pub unsafe fn load_hot_app(path: &std::path::Path) -> Result<HotApp, Box<dyn std::error::Error>> {
    let (entries, library) = unsafe { platform_core::guest::open_guest::<HotEntries>(path) }
        .map_err(|why| format!("{}: {why}", path.display()))?;
    Ok(HotApp::start(entries, Some(library)))
}

/// The host's font database, for a library's copy of the shaper to defer to: two copies of the shaper cannot share one database, so the library loads what the host's was loaded from, and a face the app adds while it runs is added to the host's, which draws its text, before the library loads it as well.
#[cfg(feature = "shaper")]
fn served_fonts() -> Option<std::sync::Arc<dyn renderer_core::FontHost>> {
    Some(std::sync::Arc::new(renderer_text::fonts::Served))
}

#[cfg(not(feature = "shaper"))]
fn served_fonts() -> Option<std::sync::Arc<dyn renderer_core::FontHost>> {
    None
}

#[cfg(feature = "desktop-bare")]
use telar_project::protocol::{
    BUILD_ERROR_PREFIX, FOCUS_LINE, GOTO_PREFIX, HOT_RELOAD_PREFIX, HOT_REPLIES_LINE,
    HOT_TOKEN_VAR, unescape_lines,
};
#[cfg(feature = "dev")]
pub(crate) use telar_project::protocol::{HotOutcome, HotReply};

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
#[cfg(feature = "desktop-bare")]
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

#[cfg(feature = "dev")]
pub(crate) struct HotLink {
    pub(crate) events: std::sync::mpsc::Receiver<HotEvent>,
    pub(crate) replies: HotReplies,
}

/// Sending never blocks, and a reply with no channel left to reach is dropped.
#[cfg(feature = "dev")]
pub(crate) struct HotReplies(std::sync::mpsc::Sender<HotReply>);

#[cfg(feature = "dev")]
impl HotReplies {
    #[cfg(any(feature = "desktop-bare", test))]
    pub(crate) fn new(sender: std::sync::mpsc::Sender<HotReply>) -> Self {
        Self(sender)
    }

    pub(crate) fn send(&self, reply: HotReply) {
        let _ = self.0.send(reply);
    }
}

/// Connects to the cargo-telar TCP loopback channel (it binds the port and passes it via `TELAR_HOT_PORT`, and its secret via [`HOT_TOKEN_VAR`]), forwards line-delimited hot events and writes back what is sent through [`HotLink::replies`]. TCP instead of a unix socket so the same code path works on non-Unix hosts.
#[cfg(feature = "desktop-bare")]
pub(crate) fn listen_hot_reload(port: u16) -> HotLink {
    use std::io::BufRead;
    let (tx, events) = std::sync::mpsc::channel();
    let (reply_tx, replies) = std::sync::mpsc::channel();
    let link = HotLink {
        events,
        replies: HotReplies::new(reply_tx),
    };
    let Ok(token) = std::env::var(HOT_TOKEN_VAR) else {
        tracing::error!(
            "hot reload is off: {HOT_TOKEN_VAR} is not set, so the channel cargo-telar opened would not take this app"
        );
        return link;
    };
    std::thread::Builder::new()
        .name("telar-hot-reload".to_string())
        .spawn(move || {
            let stream = match connect_hot_channel(port, &token) {
                Ok(stream) => stream,
                Err(e) => {
                    tracing::error!("hot reload channel connect failed (port {port}): {e}");
                    return;
                }
            };
            match stream.try_clone() {
                Ok(writer) => answer_reloads(writer, replies),
                Err(e) => tracing::error!(
                    "cargo-telar will not hear whether a reload loads: the hot reload channel could not be shared: {e}"
                ),
            }
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
    link
}

/// Writes each reply from a thread of its own, so a `cargo telar dev` that reads none, one built before replies, can fill the socket without ever stalling the app's loop.
#[cfg(feature = "desktop-bare")]
fn answer_reloads(mut stream: std::net::TcpStream, replies: std::sync::mpsc::Receiver<HotReply>) {
    use std::io::Write;
    std::thread::Builder::new()
        .name("telar-hot-replies".to_string())
        .spawn(move || {
            for reply in replies {
                if writeln!(stream, "{}", reply.line()).is_err() {
                    break;
                }
            }
        })
        .ok();
}

/// The channel, opened with `token` and then [`HOT_REPLIES_LINE`]: cargo-telar adopts no connection that does not open with the token, and waits for replies only from an app that says it sends them.
#[cfg(feature = "desktop-bare")]
fn connect_hot_channel(port: u16, token: &str) -> std::io::Result<std::net::TcpStream> {
    use std::io::Write;
    let mut attempts = 0;
    // cargo-telar binds before spawning us, but retry briefly in case it is mid-rebuild.
    let mut stream = loop {
        match std::net::TcpStream::connect(("127.0.0.1", port)) {
            Ok(stream) => break stream,
            Err(e) if attempts >= 20 => return Err(e),
            Err(_) => {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }
    };
    write!(stream, "{token}\n{HOT_REPLIES_LINE}\n")?;
    Ok(stream)
}

#[cfg(test)]
#[path = "hot_test.rs"]
mod tests;
