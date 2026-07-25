#[cfg(feature = "dev")]
pub struct HotApp {
    // inner must be declared first so it drops before _lib (Rust drops fields in declaration order)
    inner: Box<dyn crate::app::App>,
    _lib: libloading::Library,
}

/// The host's handle on a tree the dylib mounted and owns: an opaque pointer plus the shims to drive it. The
/// function pointers are copied out of the library once (plain `fn` pointers, not borrowed `Symbol`s) so this
/// handle carries no lifetime; it is valid for as long as the library stays mapped, which the runner guarantees
/// by dropping the tree before it replaces the app.
#[cfg(feature = "dev")]
struct HotTreeHandle {
    ptr: *mut crate::tree::HotTree,
    on_event: unsafe extern "Rust" fn(*mut crate::tree::HotTree, &platform_core::Event) -> bool,
    paint: unsafe extern "Rust" fn(*mut crate::tree::HotTree) -> Vec<renderer_core::DrawCommand>,
    is_dirty: unsafe extern "Rust" fn(*mut crate::tree::HotTree) -> bool,
    generation: unsafe extern "Rust" fn(*mut crate::tree::HotTree) -> u64,
    walk: unsafe extern "Rust" fn(*mut crate::tree::HotTree) -> Vec<ui_tree::SegmentNodeInfo>,
    release: unsafe extern "Rust" fn(*mut crate::tree::HotTree),
}

#[cfg(feature = "dev")]
impl HotTreeHandle {
    /// Resolves every shim up front and mounts the tree inside the dylib. `None` when any symbol is missing (a
    /// dylib built before app-side mounting existed), so the caller can fall back to mounting on the host side.
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

    // Mount inside the dylib, where the app's signals live: a tree mounted out here would register its segment
    // effects in the host's reactive runtime and never subscribe to anything the app writes (see `crate::tree`).
    // A dylib built before these shims existed falls back to the old host-side mount, which still runs — driven
    // by the force-tick workaround — rather than failing to start.
    fn mount(&mut self) -> Box<dyn crate::tree::UiTree> {
        match HotTreeHandle::mount(&self._lib, self.inner.as_ref()) {
            Some(handle) => Box::new(handle),
            None => {
                tracing::warn!(
                    "dylib exports no app-side tree mount; falling back to host-side mounting (rebuild it)"
                );
                Box::new(crate::tree::LocalTree::new(self.inner.root()))
            }
        }
    }

    fn clear_color(&self) -> Option<renderer_core::Color> {
        self.inner.clear_color()
    }

    fn on_frame(&mut self, ctx: &mut crate::app_context::AppCtx) {
        self.inner.on_frame(ctx)
    }

    fn hot_snapshot(&self) -> Option<String> {
        // Missing symbol (dylib built before hot state existed) degrades to no preservation.
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

    // Resolved per call rather than cached: this is a dev-only path (never compiled into release builds), the symbol lookup is a cheap hashmap hit, and per-call resolution avoids storing a `Symbol` borrowed from `_lib` inside the same struct. Missing symbol (dylib built before hot motion existed) degrades to a no-op: the host's own motion-core copy is a separate, empty registry, so ticking it would accomplish nothing useful.
    fn motion_tick(&self, now: std::time::Instant) {
        if let Ok(tick) = unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn(std::time::Instant)>(b"_rsx_hot_motion_tick\0")
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

    // Batch the dylib's own reactive runtime (separate from the host's) across event dispatch. Missing symbol (dylib built before this existed) degrades to a no-op: without it the app runs as before, just without the mid-dispatch flush protection.
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

    // Relayout the dylib's own layout runtime (separate from the host's) so a reactive list change is laid
    // out before the frame composes. Missing symbol (dylib built before this existed) degrades to a no-op.
    fn relayout(&self) {
        if let Ok(relayout) = unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn()>(b"_rsx_hot_relayout\0")
        } {
            unsafe { relayout() }
        }
    }

    // Consult the dylib's own overlay registry (separate thread-local from the host's): `overlay` widgets
    // register in the dylib where the view is built, so a modal's priority routing / background blocking
    // must be driven across this boundary. Missing symbol (dylib built before this existed) degrades to
    // `false` — the event falls through to the tree walk, as before this feature.
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

    // Drain window commands from the dylib's own thread-local queue (separate from the host's): a title bar's
    // `on_press` pushes into the dylib's platform-core copy, so the host must drain it across this boundary to
    // apply drag/minimize/maximize/close. Missing symbol (dylib built before this existed) degrades to an
    // empty vec — window controls are simply inert until the dylib is rebuilt.
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

    // Write the OS light/dark preference into the dylib's own theme runtime (separate from the host's), where
    // the `follow_system` effect lives. Missing symbol (dylib built before this existed) degrades to a no-op.
    fn set_system_dark(&self, dark: bool) {
        if let Ok(set) = unsafe {
            self._lib
                .get::<unsafe extern "Rust" fn(bool)>(b"_rsx_hot_set_system_dark\0")
        } {
            unsafe { set(dark) }
        }
    }
}

#[cfg(feature = "dev")]
pub fn load_hot_app(path: &std::path::Path) -> Result<HotApp, Box<dyn std::error::Error>> {
    // Copy to a unique path before dlopen. On Linux, dlopen caches loaded libraries by (device, inode). If the linker writes the new .so in-place (same inode), dlopen returns the already-loaded old handle instead of the fresh build. Copying creates a new inode, guaranteeing a fresh load. Unlinking after dlopen is safe: the kernel keeps the inode alive via the mapping until the Library is dropped.
    let unique = path.with_file_name(format!(
        ".hot-{}.so",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::copy(path, &unique)?;
    // RUNTIME and THEME use trivially-destructible TLS types (no Drop impl), so no TLS destructors are registered in the dylib. dlclose without RTLD_NODELETE is safe.
    #[cfg(unix)]
    let lib_result = unsafe {
        libloading::os::unix::Library::open(
            Some(unique.as_os_str()),
            libc::RTLD_NOW | libc::RTLD_LOCAL,
        )
        .map(libloading::Library::from)
    };
    #[cfg(not(unix))]
    let lib_result = unsafe { libloading::Library::new(&unique) };
    let _ = std::fs::remove_file(&unique);
    let lib = lib_result?;
    let create: libloading::Symbol<unsafe extern "Rust" fn() -> Box<dyn crate::app::App>> =
        unsafe { lib.get(b"_rsx_hot_create_app\0") }?;
    let inner = unsafe { create() };
    Ok(HotApp { inner, _lib: lib })
}

#[cfg(feature = "dev")]
pub enum HotEvent {
    Reload(std::path::PathBuf),
    BuildError(String),
}

/// Connects to the cargo-rsx TCP loopback channel (it binds the port and passes it via
/// `RSX_HOT_PORT`) and forwards line-delimited hot events. TCP instead of a unix socket so the
/// same code path works on non-Unix hosts.
#[cfg(feature = "dev")]
pub fn listen_hot_reload(port: u16) -> std::sync::mpsc::Receiver<HotEvent> {
    use std::io::BufRead;
    use std::net::TcpStream;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("rsx-hot-reload".to_string())
        .spawn(move || {
            // cargo-rsx binds before spawning us, but retry briefly in case it is mid-rebuild.
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
                    HotEvent::BuildError(msg.replace(" | ", "\n"))
                } else {
                    // Legacy: bare path (no prefix) — treat as reload
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
