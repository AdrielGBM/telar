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
impl crate::app::App for HotApp {
    fn root(&self) -> Box<dyn ui_core::Component> {
        self.inner.root()
    }

    // Mounted inside the dylib, where the app's signals live: a tree mounted out here would register its segment effects in the host's runtime and never subscribe to anything the app writes. A dylib too old to export them has no fallback — the host-side mount only worked while a force-tick re-ran every segment.
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

    fn on_frame(&mut self, ctx: &mut crate::app_context::AppCtx) {
        self.inner.on_frame(ctx)
    }

    fn hot_snapshot(&self) -> Option<String> {
        // Missing symbol (a dylib built before hot state existed) degrades to no preservation.
        let snapshot: libloading::Symbol<unsafe extern "Rust" fn() -> String> =
            unsafe { self._lib.get(b"_rsx_hot_snapshot\0") }.ok()?;
        Some(unsafe { snapshot() })
    }

    fn hot_restore(&self, blob: &str) {
        if let Ok(restore) = unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn(&str)>(b"_rsx_hot_restore\0")
        } {
            unsafe { restore(blob) }
        }
    }

    // Resolved per call rather than cached: a dev-only path where the lookup is a cheap hashmap hit, and this avoids storing a `Symbol` borrowed from `_lib` in the same struct. A missing symbol is a no-op, since the host's own motion-core copy is a separate, empty registry.
    fn motion_tick(&self, now: web_time::Instant) {
        if let Ok(tick) = unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn(web_time::Instant)>(b"_rsx_hot_motion_tick\0")
        } {
            unsafe { tick(now) }
        }
    }

    fn motion_has_active(&self) -> bool {
        let Ok(active) = (unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn() -> bool>(b"_rsx_hot_motion_active\0")
        }) else {
            return false;
        };
        unsafe { active() }
    }

    fn motion_has_continuous(&self) -> bool {
        let Ok(continuous) = (unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn() -> bool>(b"_rsx_hot_motion_continuous\0")
        }) else {
            return false;
        };
        unsafe { continuous() }
    }

    // The dylib's reactive runtime is separate from the host's. A missing symbol degrades to a no-op: the app runs as before, without the mid-dispatch flush protection.
    fn begin_event_batch(&self) {
        if let Ok(begin) = unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn()>(b"_rsx_hot_begin_batch\0")
        } {
            unsafe { begin() }
        }
    }

    fn end_event_batch(&self) {
        if let Ok(end) = unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn()>(b"_rsx_hot_end_batch\0")
        } {
            unsafe { end() }
        }
    }

    // The dylib's own layout runtime, so a reactive list change is laid out before the frame composes. A missing symbol degrades to a no-op.
    fn relayout(&self) {
        if let Ok(relayout) = unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn()>(b"_rsx_hot_relayout\0")
        } {
            unsafe { relayout() }
        }
    }

    // `overlay` widgets register in the dylib where the view is built, so a modal's priority routing must be driven across this boundary. A missing symbol degrades to `false` and the event falls through.
    fn dispatch_overlays(&self, event: &platform_core::Event) -> bool {
        let Ok(dispatch) = (unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn(&platform_core::Event) -> bool>(
                    b"_rsx_hot_dispatch_overlays\0",
                )
        }) else {
            return false;
        };
        unsafe { dispatch(event) }
    }

    // A title bar's `on_press` pushes into the dylib's platform-core copy, so the host drains it across this boundary. A missing symbol degrades to an empty vec: window controls are inert until the dylib is rebuilt.
    fn drain_window_commands(&self) -> Vec<platform_core::WindowCommand> {
        let Ok(drain) = (unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn() -> Vec<platform_core::WindowCommand>>(
                    b"_rsx_hot_drain_window_commands\0",
                )
        }) else {
            return Vec::new();
        };
        unsafe { drain() }
    }

    // The `follow_system` effect lives in the dylib's theme runtime. A missing symbol degrades to a no-op.
    fn set_system_dark(&self, dark: bool) {
        if let Ok(set) = unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn(bool)>(b"_rsx_hot_set_system_dark\0")
        } {
            unsafe { set(dark) }
        }
    }

    // `spawn_task` registered their callbacks in the dylib's own reactive-core thread-local, so the host must drain it across this boundary; its own copy is empty. A missing symbol degrades to a no-op.
    fn drain_tasks(&self) {
        if let Ok(drain) = unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn()>(b"_rsx_hot_drain_tasks\0")
        } {
            unsafe { drain() }
        }
    }

    // So a worker finishing inside the dylib can run a frame. A missing symbol degrades to a no-op: results then wait for the next input event.
    fn install_task_waker(&self, waker: crate::app_context::RedrawWaker) {
        if let Ok(install) = unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn(crate::app_context::RedrawWaker)>(
                    b"_rsx_hot_install_task_waker\0",
                )
        } {
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
    let lib_result = crate::dylib::open(&unique);
    let _ = std::fs::remove_file(&unique);
    let lib = lib_result?;
    let create: libloading::Symbol<unsafe extern "Rust" fn() -> Box<dyn crate::app::App>> =
        unsafe { lib.get(b"_rsx_hot_create_app\0") }?;
    let inner = unsafe { create() };
    Ok(HotApp { inner, _lib: lib })
}

#[cfg(feature = "dev")]
/// What `cargo telar dev` sends the running app: a rebuild landed, or a build failed.
pub enum HotEvent {
    Reload(std::path::PathBuf),
    BuildError(String),
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
                let event = if let Some(path_str) = line.strip_prefix("hot:") {
                    HotEvent::Reload(std::path::PathBuf::from(path_str))
                } else if let Some(msg) = line.strip_prefix("err:") {
                    HotEvent::BuildError(unescape_lines(msg))
                } else {
                    // Legacy bare path, with no prefix.
                    HotEvent::Reload(std::path::PathBuf::from(line))
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
mod tests {
    use super::*;

    /// The build error shown in the window is a rustc-shaped code frame, and a frame is mostly `|` — which is exactly what the old protocol substituted for a newline. Round-tripping one proves the frame survives instead of being cut apart at every gutter.
    #[test]
    fn a_code_frame_survives_the_hot_reload_channel() {
        let frame =
            "error: mismatched types\n --> src/home.rsx:4\n  |\n4 | text \"{n}\" size:no\n  |\n";
        let escaped = frame
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "");
        assert!(
            !escaped.contains('\n'),
            "the wire carries one line per event"
        );
        assert_eq!(unescape_lines(&escaped), frame);
    }

    /// A backslash in the message (a Windows path, an escaped quote rustc quoted back) is not a line break.
    #[test]
    fn a_literal_backslash_is_not_read_as_an_escape() {
        let message = "cannot find `C:\\src\\home.rsx`";
        let escaped = message.replace('\\', "\\\\").replace('\n', "\\n");
        assert_eq!(unescape_lines(&escaped), message);
    }
}
