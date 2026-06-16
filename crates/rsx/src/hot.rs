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
}

#[cfg(feature = "dev")]
pub fn load_hot_app(path: &std::path::Path) -> Result<HotApp, Box<dyn std::error::Error>> {
    // Copy to a unique path before dlopen. On Linux, dlopen caches loaded libraries by
    // (device, inode). If the linker writes the new .so in-place (same inode), dlopen
    // returns the already-loaded old handle instead of the fresh build. Copying creates a
    // new inode, guaranteeing a fresh load. Unlinking after dlopen is safe: the kernel
    // keeps the inode alive via the mapping until the Library is dropped.
    let unique = path.with_file_name(format!(
        ".hot-{}.so",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::copy(path, &unique)?;
    // RTLD_NODELETE prevents the library from being unmapped at dlclose. Without it, thread-local
    // destructors registered by the dylib (RUNTIME, THEME, etc.) keep dangling pointers to unmapped
    // code and corrupt the heap when the main thread exits after dlclose.
    #[cfg(unix)]
    let lib_result = unsafe {
        libloading::os::unix::Library::open(
            Some(unique.as_os_str()),
            libc::RTLD_NOW | libc::RTLD_LOCAL | libc::RTLD_NODELETE,
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

#[cfg(all(feature = "dev", unix))]
pub fn listen_hot_reload(socket_path: &str) -> std::sync::mpsc::Receiver<std::path::PathBuf> {
    use std::io::BufRead;
    use std::os::unix::net::UnixListener;
    let _ = std::fs::remove_file(socket_path);
    let (tx, rx) = std::sync::mpsc::channel();
    let socket_path = socket_path.to_owned();
    std::thread::Builder::new()
        .name("rsx-hot-reload".to_string())
        .spawn(move || {
            let Ok(listener) = UnixListener::bind(&socket_path) else {
                eprintln!("[rsx] Hot reload socket bind failed");
                return;
            };
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut buf = String::new();
                let mut reader = std::io::BufReader::new(&mut stream);
                if reader.read_line(&mut buf).is_ok() {
                    let path = std::path::PathBuf::from(buf.trim());
                    if tx.send(path).is_err() {
                        break;
                    }
                }
            }
        })
        .ok();
    rx
}
