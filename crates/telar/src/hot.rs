//! [`HotApp`]: an application loaded from a dylib, and the symbols the host drives its own runtime through.

#[cfg(feature = "dev")]
/// An application loaded from a dylib, driven through the symbols it exports.
pub struct HotApp {
    // Declared before `_lib` so it drops first: Rust drops fields in declaration order.
    inner: Box<dyn crate::app::App>,
    _lib: libloading::Library,
}

/// The host's handle on a tree the dylib mounted and owns: an opaque pointer plus the shims to drive it. The function pointers are copied out of the library once (plain `fn` pointers, not borrowed `Symbol`s) so this handle carries no lifetime; it is valid for as long as the library stays mapped, which the runner guarantees by dropping the tree before it replaces the app.
#[cfg(feature = "dev")]
struct HotTreeHandle {
    ptr: *mut crate::tree::HotTree,
    on_event: unsafe extern "Rust" fn(*mut crate::tree::HotTree, &platform_core::Event) -> bool,
    paint: unsafe extern "Rust" fn(*mut crate::tree::HotTree) -> Vec<renderer_core::DrawCommand>,
    is_dirty: unsafe extern "Rust" fn(*mut crate::tree::HotTree) -> bool,
    generation: unsafe extern "Rust" fn(*mut crate::tree::HotTree) -> u64,
    walk: unsafe extern "Rust" fn(*mut crate::tree::HotTree) -> Vec<ui_tree::SegmentNodeInfo>,
    release: unsafe extern "Rust" fn(*mut crate::tree::HotTree),
    /// Absent in a dylib built before the input registries were fed on this side; the tree still runs, it just leaves `key_pressed` answering for longer than a frame.
    end_frame: Option<unsafe extern "Rust" fn(*mut crate::tree::HotTree)>,
    /// Absent in a dylib built before the overlay layer covered the tree; a move an overlay takes then never reaches the tree, and a box hovered under the overlay keeps its hover.
    on_covered_event:
        Option<unsafe extern "Rust" fn(*mut crate::tree::HotTree, &platform_core::Event)>,
}

#[cfg(feature = "dev")]
impl HotTreeHandle {
    /// Resolves every shim up front and mounts the tree inside the dylib. `None` when any symbol is missing (a dylib built before app-side mounting existed), so the caller can fall back to mounting on the host side.
    fn mount(lib: &libloading::Library, app: &dyn crate::app::App) -> Option<Self> {
        unsafe {
            let mount: libloading::Symbol<
                unsafe extern "Rust" fn(&dyn crate::app::App) -> *mut crate::tree::HotTree,
            > = lib.get(b"_rsx_hot_tree_mount\0").ok()?;
            let handle = Self {
                ptr: mount(app),
                on_event: *lib.get(b"_rsx_hot_tree_on_event\0").ok()?,
                paint: *lib.get(b"_rsx_hot_tree_paint\0").ok()?,
                is_dirty: *lib.get(b"_rsx_hot_tree_dirty\0").ok()?,
                generation: *lib.get(b"_rsx_hot_tree_generation\0").ok()?,
                walk: *lib.get(b"_rsx_hot_tree_walk\0").ok()?,
                release: *lib.get(b"_rsx_hot_tree_release\0").ok()?,
                end_frame: lib
                    .get(b"_rsx_hot_tree_end_frame\0")
                    .ok()
                    .map(|symbol| *symbol),
                on_covered_event: lib
                    .get(b"_rsx_hot_tree_on_covered_event\0")
                    .ok()
                    .map(|symbol| *symbol),
            };
            Some(handle)
        }
    }
}

#[cfg(feature = "dev")]
impl crate::tree::UiTree for HotTreeHandle {
    fn on_event(&mut self, event: &platform_core::Event) -> ui_core::EventResult {
        if unsafe { (self.on_event)(self.ptr, event) } {
            ui_core::EventResult::Handled
        } else {
            ui_core::EventResult::Ignored
        }
    }

    fn on_covered_event(&mut self, event: &platform_core::Event) {
        if let Some(dispatch) = self.on_covered_event {
            unsafe { dispatch(self.ptr, event) }
        }
    }

    fn frame(&self) -> crate::tree::Frame<'_> {
        crate::tree::Frame::Owned(unsafe { (self.paint)(self.ptr) })
    }

    fn end_frame(&self) {
        if let Some(end) = self.end_frame {
            unsafe { end(self.ptr) }
        }
    }

    fn is_dirty(&self) -> bool {
        unsafe { (self.is_dirty)(self.ptr) }
    }

    fn generation(&self) -> u64 {
        unsafe { (self.generation)(self.ptr) }
    }

    fn walk(&self, out: &mut Vec<ui_tree::SegmentNodeInfo>) {
        out.extend(unsafe { (self.walk)(self.ptr) });
    }
}

#[cfg(feature = "dev")]
impl Drop for HotTreeHandle {
    fn drop(&mut self) {
        unsafe { (self.release)(self.ptr) };
    }
}

#[cfg(feature = "dev")]
impl HotApp {
    /// Resolved per call: the lookup is a cheap hashmap hit on a dev-only path, and caching would store a `Symbol` borrowing `_lib` in the same struct. `None` for a dylib built before the export existed, which each caller degrades past.
    fn symbol<F>(&self, name: &[u8]) -> Option<libloading::Symbol<'_, F>> {
        unsafe { self._lib.get::<F>(name) }.ok()
    }

    fn dylib_answers(&self, name: &[u8]) -> bool {
        self.symbol::<unsafe extern "Rust" fn() -> bool>(name)
            .is_some_and(|answer| unsafe { answer() })
    }
}

#[cfg(feature = "dev")]
impl crate::app_runtime::AppRuntime for HotApp {
    // The window a hot-reloaded app asks for is the one its own `app!` invocation names, which lives on the far side of the boundary.
    fn window_config(&self) -> Option<platform_core::WindowConfig> {
        self.inner.window_config()
    }

    // Mounted inside the dylib, where the app's signals live: a tree mounted out here would register its segment effects in the host's runtime and never subscribe to anything the app writes.
    fn mount(&mut self) -> Box<dyn crate::tree::UiTree> {
        match HotTreeHandle::mount(&self._lib, self.inner.as_ref()) {
            Some(handle) => Box::new(handle),
            None => panic!(
                "this dylib exports no app-side tree mount — rebuild it against the current telar"
            ),
        }
    }

    fn clear_color(&self) -> Option<renderer_core::Color> {
        self.inner.clear_color()
    }

    fn on_frame(&mut self, ctx: &mut platform_core::AppCtx) {
        self.inner.on_frame(ctx)
    }

    fn hot_snapshot(&self) -> Option<String> {
        let snapshot =
            self.symbol::<unsafe extern "Rust" fn() -> String>(b"_rsx_hot_snapshot\0")?;
        Some(unsafe { snapshot() })
    }

    fn hot_restore(&self, blob: &str) {
        if let Some(restore) = self.symbol::<unsafe extern "Rust" fn(&str)>(b"_rsx_hot_restore\0") {
            unsafe { restore(blob) }
        }
    }

    // The app's motion registry is the dylib's motion-core copy; the host's animates what the host draws itself, the devtools overlay.
    fn motion_tick(&self, now: web_time::Instant) {
        motion_core::tick(now);
        if let Some(tick) =
            self.symbol::<unsafe extern "Rust" fn(web_time::Instant)>(b"_rsx_hot_motion_tick\0")
        {
            unsafe { tick(now) }
        }
    }

    fn motion_has_active(&self) -> bool {
        motion_core::has_active() || self.dylib_answers(b"_rsx_hot_motion_active\0")
    }

    fn motion_has_continuous(&self) -> bool {
        motion_core::has_continuous() || self.dylib_answers(b"_rsx_hot_motion_continuous\0")
    }

    fn begin_event_batch(&self) {
        if let Some(begin) = self.symbol::<unsafe extern "Rust" fn()>(b"_rsx_hot_begin_batch\0") {
            unsafe { begin() }
        }
    }

    fn end_event_batch(&self) {
        if let Some(end) = self.symbol::<unsafe extern "Rust" fn()>(b"_rsx_hot_end_batch\0") {
            unsafe { end() }
        }
    }

    fn relayout(&self) {
        if let Some(relayout) = self.symbol::<unsafe extern "Rust" fn()>(b"_rsx_hot_relayout\0") {
            unsafe { relayout() }
        }
    }

    // `overlay` widgets register in the dylib where the view is built, so a modal's priority routing must be driven across this boundary.
    fn dispatch_overlays(&self, event: &platform_core::Event) -> bool {
        self.symbol::<unsafe extern "Rust" fn(&platform_core::Event) -> bool>(
            b"_rsx_hot_dispatch_overlays\0",
        )
        .is_some_and(|dispatch| unsafe { dispatch(event) })
    }

    // The controls and their rects live in the dylib's registries, which no host-side reading can see, and the dylib exports no snapshot of its own yet.
    fn access_snapshot(
        &self,
        _frame: &[renderer_core::DrawCommand],
    ) -> Vec<platform_core::AccessNode> {
        Vec::new()
    }

    // A title bar's `on_press` pushes into the dylib's platform-core copy, so the host drains it across this boundary.
    fn drain_window_commands(&self) -> Vec<platform_core::WindowCommand> {
        self.symbol::<unsafe extern "Rust" fn() -> Vec<platform_core::WindowCommand>>(
            b"_rsx_hot_drain_window_commands\0",
        )
        .map(|drain| unsafe { drain() })
        .unwrap_or_default()
    }

    // The dylib reads its own copy of the store; the host's copy is kept too, for the devtools that live on this side.
    fn set_system_preferences(&self, preferences: &platform_core::SystemPreferences) {
        preferences_core::set_system_preferences(preferences.clone());
        if let Some(set) = self
            .symbol::<unsafe extern "Rust" fn(&platform_core::SystemPreferences)>(
                b"_rsx_hot_set_system_preferences\0",
            )
        {
            unsafe { set(preferences) }
        }
    }

    fn set_surface_size(&self, size: geometry_core::Size) {
        if let Some(set) =
            self.symbol::<unsafe extern "Rust" fn(f32, f32)>(b"_rsx_hot_set_surface_size\0")
        {
            unsafe { set(size.width, size.height) }
        }
    }

    fn set_safe_area_insets(&self, insets: geometry_core::Insets) {
        if let Some(set) = self.symbol::<unsafe extern "Rust" fn(f32, f32, f32, f32)>(
            b"_rsx_hot_set_safe_area_insets\0",
        ) {
            unsafe { set(insets.top, insets.right, insets.bottom, insets.left) }
        }
    }

    fn set_location_history(&self, history: &[platform_core::Location]) {
        if let Some(set) = self.symbol::<unsafe extern "Rust" fn(&[platform_core::Location])>(
            b"_rsx_hot_set_location_history\0",
        ) {
            unsafe { set(history) }
        }
    }

    fn open_title(&self, app: &str, showing: &str) {
        if let Some(open) =
            self.symbol::<unsafe extern "Rust" fn(&str, &str)>(b"_rsx_hot_open_title\0")
        {
            unsafe { open(app, showing) }
        }
    }

    fn open_font_family(&self, family: Option<&str>) {
        if let Some(open) =
            self.symbol::<unsafe extern "Rust" fn(Option<&str>)>(b"_rsx_hot_open_font_family\0")
        {
            unsafe { open(family) }
        }
    }

    // Its dialogs and its history are the dylib's.
    fn navigate_back(&self) -> bool {
        self.symbol::<unsafe extern "Rust" fn() -> bool>(b"_rsx_hot_navigate_back\0")
            .is_some_and(|back| unsafe { back() })
    }

    // `spawn_task` registers its callback in the reactive-core copy of whoever called it, so both sides are drained.
    fn drain_tasks(&self) {
        reactive_core::drain_tasks();
        if let Some(drain) = self.symbol::<unsafe extern "Rust" fn()>(b"_rsx_hot_drain_tasks\0") {
            unsafe { drain() }
        }
    }

    fn fire_timers(&self) {
        reactive_core::fire_timers();
        if let Some(fire) = self.symbol::<unsafe extern "Rust" fn()>(b"_rsx_hot_fire_timers\0") {
            unsafe { fire() }
        }
    }

    // The loop wakes for whichever side's timer comes due first.
    fn until_next_timer(&self) -> Option<std::time::Duration> {
        let host = reactive_core::until_next_timer();
        let app = self
            .symbol::<unsafe extern "Rust" fn() -> Option<std::time::Duration>>(
                b"_rsx_hot_until_next_timer\0",
            )
            .and_then(|until| unsafe { until() });
        match (host, app) {
            (Some(host), Some(app)) => Some(host.min(app)),
            (host, app) => host.or(app),
        }
    }

    fn install_task_waker(&self, waker: platform_core::RedrawWaker) {
        let host = waker.clone();
        reactive_core::set_task_waker(move || host.wake());
        if let Some(install) = self.symbol::<unsafe extern "Rust" fn(platform_core::RedrawWaker)>(
            b"_rsx_hot_install_task_waker\0",
        ) {
            unsafe { install(waker) }
        }
    }
}

#[cfg(feature = "dev")]
/// Copies the dylib to a unique path and dlopens it, so a rebuild is never served from the loader's cache.
pub fn load_hot_app(path: &std::path::Path) -> Result<HotApp, Box<dyn std::error::Error>> {
    // dlopen caches loaded libraries by (device, inode), so a linker writing the new .so in place would return the already-loaded old handle. Unlinking after dlopen is safe: the mapping keeps the inode alive.
    let unique = path.with_file_name(format!(
        ".hot-{}.so",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::copy(path, &unique)?;
    // `RUNTIME` and `THEME` use trivially-destructible TLS types, so the dylib registers no TLS destructors and `dlclose` without `RTLD_NODELETE` is safe.
    let lib_result = platform_core::guest::open(&unique);
    let _ = std::fs::remove_file(&unique);
    let lib = lib_result?;
    // Before the app is made, so whatever its setup or its first build reaches for is already there. A library built before the symbol existed keeps its empty stores.
    if let Ok(install) =
        unsafe { lib.get::<unsafe extern "Rust" fn(HostServices)>(b"_rsx_hot_install_services\0") }
    {
        unsafe { install(HostServices::installed()) }
    }
    let create: libloading::Symbol<unsafe extern "Rust" fn() -> Box<dyn crate::app::App>> =
        unsafe { lib.get(b"_rsx_hot_create_app\0") }?;
    let inner = unsafe { create() };
    Ok(HotApp { inner, _lib: lib })
}

/// The platform services the host installed, handed to each library it loads: the library's copies of their stores start empty, so without them a reloaded app copies to no clipboard, opens no link and shows no file dialog.
///
/// The host's backends serve both sides, so a selection the clipboard owns outlives every library swapped in under it.
#[cfg(feature = "dev")]
#[doc(hidden)]
pub struct HostServices {
    clipboard: Option<std::sync::Arc<dyn services_core::Clipboard>>,
    uri_opener: Option<std::sync::Arc<dyn services_core::UriOpener>>,
    file_dialogs: Option<std::sync::Arc<dyn services_core::FileDialogs>>,
}

#[cfg(feature = "dev")]
impl HostServices {
    /// What this side has installed.
    pub fn installed() -> Self {
        Self {
            clipboard: services_core::clipboard(),
            uri_opener: services_core::uri_opener(),
            file_dialogs: services_core::file_dialogs(),
        }
    }

    /// Installs each service on this side, as the host had it.
    pub fn install(self) {
        if let Some(clipboard) = self.clipboard {
            services_core::set_clipboard(clipboard);
        }
        if let Some(opener) = self.uri_opener {
            services_core::set_uri_opener(opener);
        }
        if let Some(dialogs) = self.file_dialogs {
            services_core::set_file_dialogs(dialogs);
        }
    }
}

#[cfg(feature = "dev")]
use telar_project::protocol::{BUILD_ERROR_PREFIX, GOTO_PREFIX, HOT_RELOAD_PREFIX};

#[cfg(feature = "dev")]
/// What `cargo telar dev` sends the running app: a rebuild landed, a build failed, or a location to open.
#[derive(Debug, PartialEq)]
pub enum HotEvent {
    Reload(std::path::PathBuf),
    BuildError(String),
    /// A location reference, as `LocationFormat::parse` reads one, to open as a link into the running app would: `cargo telar preview <ID>` moving the workshop already open.
    Goto(String),
}

/// The event one line of the channel carries, or `None` for a line with a prefix this host does not know.
#[cfg(feature = "dev")]
fn hot_event(line: &str) -> Option<HotEvent> {
    if let Some(path) = line.strip_prefix(HOT_RELOAD_PREFIX) {
        Some(HotEvent::Reload(std::path::PathBuf::from(path)))
    } else if let Some(message) = line.strip_prefix(BUILD_ERROR_PREFIX) {
        Some(HotEvent::BuildError(unescape_lines(message)))
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
