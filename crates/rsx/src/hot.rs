#[cfg(feature = "dev")]
pub struct HotApp {
    // inner must be declared first so it drops before _lib (Rust drops fields in declaration order)
    inner: Box<dyn crate::app::App>,
    _lib: libloading::Library,
}

#[cfg(feature = "dev")]
impl crate::app::App for HotApp {
    fn root(&self) -> Box<dyn ui_core::Component> {
        self.inner.root()
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
